use std::thread::JoinHandle;

use deno_core::{
  serde_v8,
  v8::{self, Local},
};

use crate::worker::{SandboxWorker, SandboxWorkerCreateOptions};

pub struct SandboxWorkerPool {
  threads: Vec<JoinHandle<()>>,
  sender: crossbeam_channel::Sender<SandboxWorkerCreateOptions>
}

impl SandboxWorkerPool {
  pub fn new(num_threads: usize) -> Self {
    let (send, recv) =
      crossbeam_channel::unbounded::<SandboxWorkerCreateOptions>();

    let mut this = Self {
      threads: Vec::new(),
      sender: send
    };

    // Spawn worker threads
    for i in 0..num_threads {
      let recv = recv.clone();
      this.threads.push(std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
          .thread_name(format!("sandbox-worker-{}", i))
          .enable_all()
          .build()
          .expect("could not build tokio runtime");

        rt.block_on(async move {
          loop {
            let local = tokio::task::LocalSet::new();

            local.run_until(async move {
              loop {
                // Receive new workers
              }
            }).await;
          }
        });
      }));
    }

    this
  }

  /// Creates a new worker and returns a promise.
  pub fn create_new_worker(
    &self,
    options: SandboxWorkerCreateOptions,
  ) -> serde_v8::Global {
    unimplemented!();
  }
}
