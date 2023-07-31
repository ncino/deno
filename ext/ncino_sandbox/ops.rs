use std::future;
use std::path::Path;

use deno_core::op;
use deno_core::serde_v8;
use deno_core::task::JoinHandle;
use deno_core::v8;
use deno_core::v8::Global;
use deno_core::v8::HandleScope;
use deno_core::v8::Local;
use deno_core::v8::Promise;
use deno_core::v8::PromiseResolver;
use deno_core::OpState;
use tokio::runtime::Handle;

use crate::pool::SandboxWorkerPool;
use crate::worker::SandboxWorker;
use crate::worker::SandboxWorkerCreateOptions;

// Ops and scripts to enable the execution of the sandbox from Deno
deno_core::extension!(hypervisor, ops = [op_run_edge_function], esm = [dir "js", "hypervisor.js"], state = |state| {
  state.put(SandboxWorkerPool::new(8));
});

// The runtime that executes in an edge-function sandbox.
deno_core::extension!(runtime, esm = [dir "js", "console.js"]);

// TODO: Define acceptable params for this function.
#[op(v8)]
fn op_run_edge_function(
  scope: &mut v8::HandleScope,
  state: &mut OpState,
  path: String,
  request_obj: serde_v8::Value<'_>,
  // TODO: Return a promise and make this run on a background thread.
) -> serde_v8::Global {
  println!("got to rust!");
  let pool: &SandboxWorkerPool = state.borrow();

  let resolver =
    PromiseResolver::new(scope).expect("could not create promise resolver");
  let promise: Local<'_, v8::Value> =
    unsafe { Local::cast::<v8::Value>(resolver.get_promise(scope).into()) };

  pool.create_new_worker(SandboxWorkerCreateOptions {
    path,
    request: *request_obj.v8_value,
  });

  let promise = v8::Global::new(scope, promise);
  promise.into()
}
