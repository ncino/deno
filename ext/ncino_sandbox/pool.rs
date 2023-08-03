use ::futures::{
  stream::futures_unordered,
  task::{waker_ref, ArcWake},
  FutureExt,
};
use anyhow::anyhow;
use cassette::Cassette;
use deno_url::op_url_get_serialization;
use std::{
  borrow::BorrowMut,
  cell::{RefCell, UnsafeCell},
  ffi::c_void,
  sync::{atomic::AtomicU64, Arc},
  thread::JoinHandle,
  time::Duration, path::Path, mem::MaybeUninit,
};
use std::{ptr::NonNull, task::Context};
use deno_core::serde_v8;

use async_channel::Receiver;
use deno_core::{
  futures::{
    future::BoxFuture,
    stream::FuturesUnordered,
    task::{AtomicWaker, WakerRef},
    StreamExt,
  },
  v8::{self, IsolateHandle},
  v8::{
    CallbackScope, Function, Global, Handle, HandleScope, OwnedIsolate,
    PromiseResolver, Value,
  },
  BufView, ModuleSpecifier, Resource, CreateRealmOptions,
};
use tokio::{
  runtime::Runtime,
  sync::{futures, oneshot, Mutex, RwLock},
};

use crate::worker::SandboxRuntime;

pub type SandboxWorkerId = u64;

pub struct SandboxWorkerCreateOptions {
  pub main_module: ModuleSpecifier,
  pub request_json: String,
  pub request_body: Vec<u8>,
  pub resolver: v8::Global<v8::PromiseResolver>,
  pub path: String
}

pub struct Worker {
  pub path: String,
  pub resolver: Global<PromiseResolver>,
  pub id: SandboxWorkerId,
  pub request: (String, Vec<u8>),

  /// SAFETY: response is valid until `should_dispose` is sent.
  pub response: Option<anyhow::Result<(String, Vec<u8>)>>,
  /// SAFETY: This will always be populated as long as the worker was received using pool.drain().
  /// In other words, you should initialize this value when you attach a Response
  pub should_dispose: MaybeUninit<oneshot::Sender<()>>
}

// SAFETY: The global resolver will always last longer than the parent isolate.
// Additionally, no values in the parent Isolate are made unless queried by the isolator
// via the drain op.
unsafe impl Send for Worker {}

pub struct SandboxOrchestrator {
  threads: Vec<JoinHandle<()>>,
  sender: async_channel::Sender<(Worker, oneshot::Sender<Worker>)>,
  next_worker_id: AtomicU64,
  futures: FuturesUnordered<BoxFuture<'static, Worker>>,
}

impl SandboxOrchestrator {
  pub fn new(num_threads: usize) -> Self {
    let (send, recv) = async_channel::bounded(100);

    let mut this = Self {
      threads: Vec::new(),
      sender: send,
      next_worker_id: AtomicU64::new(0),
      futures: FuturesUnordered::new(),
    };

    // Spawn worker threads
    for i in 0..num_threads {
      let recv = recv.clone();
      let t = std::thread::Builder::new()
        .name(format!("sandbox-worker-{}", i))
        .spawn(move || {
          let rt = tokio::runtime::Builder::new_current_thread()
            .thread_name(format!("sandbox-worker-{}", i))
            .enable_all()
            .build()
            .expect("could not build tokio runtime");

          rt.block_on(async move {
            Self::init_thread(recv).await;
          });
        })
        .unwrap();
      this.threads.push(t);
    }

    this
  }

  /// Creates a new worker and returns a channel where the response will be sent.
  pub fn create_new_worker(&self, options: SandboxWorkerCreateOptions) {
    let (send, recv) = oneshot::channel();

    let worker_id = self
      .next_worker_id
      .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let worker = Worker {
      resolver: options.resolver,
      id: worker_id,
      request: (options.request_json, options.request_body),
      response: None,
      path: options.path,
      should_dispose: MaybeUninit::uninit(),
    };

    self.sender.send_blocking((worker, send)).unwrap();

    self
      .futures
      .push(Box::pin(async move { recv.await.unwrap() }));
  }

  /// If any workers have finished gather them.
  pub fn drain(&mut self) -> Vec<Worker> {
    // FIXME: optimize this
    let mut drain = vec![];
    loop {
      let mut pending = Cassette::new(self.futures.next());
      let res = pending.poll_on();
      if let Some(maybe_worker) = res {
        if let Some(worker) = maybe_worker {
          println!("finished executing worker: {}", worker.id);
          drain.push(worker);
        } else {
          break;
        }
      } else {
        break;
      }
    }
    return drain;
  }

  async fn init_thread(mut recv: Receiver<(Worker, oneshot::Sender<Worker>)>) {
    let local = tokio::task::LocalSet::new();


    local
      .run_until(async {
        let mut futs = FuturesUnordered::new();
        loop {
          tokio::select! {
            Some((mut worker, sender)) = recv.next() => {
              println!("Received new worker: {}", worker.id);

              futs.push(async move {
                let platform = v8::V8::get_current_platform();
                let mut runtime = SandboxRuntime::new(platform);
                runtime.bootstrap(Path::new(worker.path.as_str())).await?;

                // FIXME: Do this without cloning the body buffer
                let req_clone = worker.request.clone();
                let response = runtime.run(worker.request).await?;

                worker.response = Some(Ok(response));

                let (dispose, wait_for_dispose) = oneshot::channel();
                worker.should_dispose = MaybeUninit::new(dispose);

                worker.request = req_clone;

                sender.send(worker).map_err(|_| anyhow!("could not send worker to thread"))?;

                // Race condition: Isolate may die before the other thread can respond to the http request.
                // This ensures that the isolate stays alive until the other thread can respond.
                wait_for_dispose.await.unwrap();
                return Ok(()) as anyhow::Result<()>;
              });

              // TODO: Find a way to push a microtask onto the other isolate.
            },
            Some(result) = futs.next() => {
              println!("finished a worker");
              match result {
                  Ok(()) => {},
                  Err(e) => eprintln!("Encountered an error while running worker: {}", e)
              }
            },
            else => {
              tokio::time::sleep(Duration::from_millis(10)).await;
            }
          }
        }
      })
      .await;
  }

  pub fn add_worker(&mut self, worker: Worker) {}
}
