use anyhow::anyhow;
use deno_core::futures::TryFutureExt;
use deno_core::op2;
use deno_core::url::Url;
use deno_core::v8::CallbackScope;
use deno_core::v8::Function;
use deno_core::v8::HandleScope;
use deno_core::v8::Isolate;
use deno_core::v8::IsolateHandle;
use deno_core::v8::OwnedIsolate;
use deno_core::v8::PromiseResolver;
use deno_core::v8::Value;
use deno_core::ModuleSpecifier;
use deno_core::PromiseId;
use std::borrow::BorrowMut;
use std::cell::Ref;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use deno_core::op;
use deno_core::serde_v8;
use deno_core::v8;
use deno_core::v8::Local;
use deno_core::OpState;
use deno_core::Resource;
use deno_core::ResourceId;

use crate::pool::SandboxWorkerCreateOptions;
use crate::pool::SandboxOrchestrator;
use crate::pool::SandboxWorkerId;
use crate::pool::Worker;

// Ops and scripts to enable the execution of the sandbox from Deno
deno_core::extension!(hypervisor, ops = [op_start_edge_function, op_drain_worker_queue], esm = [dir "js", "hypervisor.js"], state = |state| {
  state.put(RefCell::new(SandboxOrchestrator::new(1)));
});

// The runtime that executes in an edge-function sandbox.
deno_core::extension!(runtime,
  esm_entry_point = "ext:runtime/99_main.js",
  esm = [dir "js", "console.js", "99_main.js"]
);

#[op2]
fn op_start_edge_function<'scope>(
  scope: &mut v8::HandleScope<'scope>,
  state: Rc<RefCell<OpState>>,
  #[string] path: String,
  // Must be an instance of Request
  request_obj: v8::Local<'scope, v8::Value>,
) -> anyhow::Result<v8::Local<'scope, v8::Promise>> {
  println!("starting to run edge function");

  let resolver = v8::PromiseResolver::new(scope).unwrap();
  let promise = resolver.get_promise(scope);
  let global_resolver = v8::Global::new(scope, resolver);

  let state = state.as_ref().borrow_mut();
  let pool: &RefCell<SandboxOrchestrator> = state.borrow();
  let pool = pool.borrow();

  pool.create_new_worker(SandboxWorkerCreateOptions {
    main_module: Url::from_file_path(path.as_str()).unwrap(),
    request: *request_obj,
    resolver: global_resolver
  });

  // let value = recv.await?;

  Ok(promise)
}

#[op2]
pub fn op_drain_worker_queue(
  scope: &mut v8::HandleScope,
  state: Rc<RefCell<OpState>>,
) -> anyhow::Result<()> {
  println!("draining queue");
  let state = state.as_ref().borrow_mut();
  let pool = state.borrow::<RefCell<SandboxOrchestrator>>();
  let mut pool = pool.borrow_mut();
  let finished_workers = pool.drain();

  for worker in finished_workers {
    let response = worker.response.unwrap().unwrap().to_object(scope).unwrap();
    let resolver = worker.resolver.open(scope);
    resolver.resolve(scope, response.into());
  }

  Ok(())
}
