use std::{thread::JoinHandle, path::Path, time::Duration};

use deno_core::{
  serde_v8,
  v8::{self, Local}, futures::{Stream, io::ReadExact, StreamExt},
};

use crate::worker::{SandboxWorker, SandboxWorkerCreateOptions};

pub struct SandboxWorkerPool {
  threads: Vec<JoinHandle<()>>,
  sender: async_channel::Sender<SandboxWorkerCreateOptions>
}

impl SandboxWorkerPool {
  pub fn new(num_threads: usize) -> Self {
    let (send, recv) =
      async_channel::unbounded::<SandboxWorkerCreateOptions>();

    let mut this = Self {
      threads: Vec::new(),
      sender: send
    };

    // Spawn worker threads
    for i in 0..num_threads {
      let mut recv = recv.clone();
      this.threads.push(std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
          .thread_name(format!("sandbox-worker-{}", i))
          .enable_all()
          .build()
          .expect("could not build tokio runtime");

        rt.block_on(async move {
          loop {
            let local = tokio::task::LocalSet::new();

            local.run_until(async {
              loop {
                tokio::select! {
                  Some(options) = recv.next() => {
                    println!("Received new worker: {}", options.path);
                    // TODO: put this is a future so it can be eventually resolved.
                    Self::handle_incoming_worker(options).await;
                  },
                  else => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                  }
                }
              }
            }).await;
          }
        });
      }));
    }

    this
  }

  pub fn create_new_worker(
    &self,
    options: SandboxWorkerCreateOptions,
  ) {
    self.sender.send_blocking(options).unwrap();
  }

  async fn handle_incoming_worker(options: SandboxWorkerCreateOptions) {
    let mut worker = SandboxWorker::new();
    worker.bootstrap(Path::new(options.path.as_str())).await.expect("could not bootstrap user function");
    worker.run(options.request).await.expect("could not run user function");
  }
}
