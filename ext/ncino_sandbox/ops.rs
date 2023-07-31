use std::future;
use std::path::Path;

use deno_core::op;
use deno_core::serde_v8;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;
use tokio::sync::futures;

use crate::SandboxWorker;

// Ops and scripts to enable the execution of the sandbox from Deno
deno_core::extension!(hypervisor, ops = [op_run_edge_function], esm = [dir "js", "hypervisor.js"]);

// The runtime that executes in an edge-function sandbox.
deno_core::extension!(runtime, esm = [dir "js", "console.js"]);

// TODO: Define acceptable params for this function.
#[op(v8)]
fn op_run_edge_function(
  path: String,
  request_obj: serde_v8::Value<'_>,
  // TODO: Return a promise and make this run on a background thread.
) -> serde_v8::Global {
  println!("got to rust!");

  let rt = Runtime::new().expect("couldn't create tokio runtime");

  let res = rt.block_on(async {
    let mut sandbox = SandboxWorker::new();
    sandbox
      .bootstrap(Path::new(path.as_str()))
      .await
      .expect("could not bootstrap edge function");
    let response = sandbox
      .run(request_obj.v8_value)
      .await
      .expect("could not run edge function");
    return serde_v8::Global::from(response);
  });


  res
}
