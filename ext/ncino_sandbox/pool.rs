use std::task::Context;
use anyhow::anyhow;
use cassette::Cassette;
use deno_url::op_url_get_serialization;
use ::futures::{task::{waker_ref, ArcWake}, FutureExt, stream::futures_unordered};
use std::{
  borrow::BorrowMut,
  cell::{RefCell, UnsafeCell},
  ffi::c_void,
  sync::{atomic::AtomicU64, Arc},
  thread::JoinHandle,
  time::Duration,
};

use async_channel::Receiver;
use deno_core::{
  futures::{
    future::BoxFuture, stream::FuturesUnordered, task::{AtomicWaker, WakerRef}, StreamExt,
  },
  v8::{self, IsolateHandle},
  v8::{
    CallbackScope, Function, Global, Handle, HandleScope, OwnedIsolate,
    PromiseResolver, Value,
  },
  BufView, ModuleSpecifier, Resource,
};
use tokio::{
  runtime::Runtime,
  sync::{oneshot, Mutex, RwLock, futures},
};

use crate::worker::SandboxRuntime;

pub type SandboxWorkerId = u64;

pub struct SandboxWorkerCreateOptions {
  pub main_module: ModuleSpecifier,
  pub request: v8::Value,
  pub resolver: v8::Global<v8::PromiseResolver>,
}

#[derive(Debug)]
pub struct Worker {
  pub resolver: Global<PromiseResolver>,
  pub id: SandboxWorkerId,
  pub request: v8::Value,
  pub response: Option<anyhow::Result<v8::Value>>,
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
        request: options.request,
        response: None,
    };

    self.sender.send_blocking((worker, send)).unwrap();

    self.futures.push(Box::pin(async move {
      recv.await.unwrap()
    }));
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

    // The main JavaScript runtime that all edge functions will be called under.
    let mut runtime = SandboxRuntime::new();

    local
      .run_until(async {
        loop {
          tokio::select! {
            Some((mut worker, sender)) = recv.next() => {
              println!("Received new worker: {}", worker.id);
              // TODO: put this is a FuturesUnordered so this can handle multiple isolates at once.
              // let result = worker.call_edge_function().await;
              // TODO: Remove this logic and implement
              tokio::time::sleep(Duration::from_secs(1)).await;

              worker.response = Some(Ok(worker.request.clone()));

              sender.send(worker).expect("could not send worker");

              // resolver.resolve(&mut scope, ret);
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
