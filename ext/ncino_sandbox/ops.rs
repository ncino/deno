use anyhow::anyhow;
use deno_core::op2;
use deno_core::url::Url;
use deno_core::v8::Global;

use std::cell::RefCell;
use std::os::macos::raw;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use deno_core::op;
use deno_core::serde_v8;
use deno_core::v8;
use deno_core::v8::Local;
use deno_core::OpState;
use deno_core::Resource;
use deno_core::ResourceId;

use crate::pool::SandboxOrchestrator;
use crate::pool::SandboxWorkerCreateOptions;
use crate::pool::SandboxWorkerId;
use crate::pool::Worker;

// Ops and scripts to enable the execution of the sandbox from Deno
deno_core::extension!(hypervisor, ops = [op_start_edge_function, op_drain_worker_queue],
  esm_entry_point = "ext:hypervisor/hypervisor.js",
  esm = [dir "js", "hypervisor.js", "01_models.js"],
  state = |state| {
    state.put(RefCell::new(SandboxOrchestrator::new(1)));
  }
);

// The runtime that executes in an edge-function sandbox.
deno_core::extension!(runtime,
  esm_entry_point = "ext:runtime/99_main.js",
  esm = [dir "js", "01_models.js", "02_console.js", "99_main.js"]
);

#[op2]
fn op_start_edge_function<'scope>(
  scope: &mut v8::HandleScope<'scope>,
  state: Rc<RefCell<OpState>>,
  #[string] path: String,
  // Must be an instance of Request
  #[string] request_model: String,
  #[buffer(copy)] body_array_buf: Vec<u8>,
) -> anyhow::Result<v8::Local<'scope, v8::Promise>> {
  let resolver = v8::PromiseResolver::new(scope).unwrap();
  let promise = resolver.get_promise(scope);
  let global_resolver = v8::Global::new(scope, resolver);

  let state = state.as_ref().borrow_mut();
  let pool: &RefCell<SandboxOrchestrator> = state.borrow();
  let pool = pool.borrow();

  pool.create_new_worker(SandboxWorkerCreateOptions {
    main_module: Url::from_file_path(path.as_str()).unwrap(),
    request_json: request_model,
    request_body: body_array_buf,
    resolver: global_resolver,
    path,
  });

  Ok(promise)
}

#[op2]
pub fn op_drain_worker_queue(
  scope: &mut v8::HandleScope,
  state: Rc<RefCell<OpState>>,
) -> anyhow::Result<()> {
  let state = state.as_ref().try_borrow_mut()?;
  let pool = state.borrow::<RefCell<SandboxOrchestrator>>();
  let mut pool = pool.try_borrow_mut()?;
  let finished_workers = pool.drain();

  for worker in finished_workers {
      let response = worker.response.ok_or_else(|| {
        anyhow!(
          "called drain on a worker before it had responded without any errors"
        )
      })??;
    let resolver = worker.resolver.open(scope);

    let value = v8::Object::new(scope);
    let response_json_key = v8::String::new_external_onebyte_static(scope, b"json").unwrap();
    let response_json = v8::String::new(scope, response.0.as_str()).ok_or_else(|| anyhow!("could not create new object for response"))?;
    value.set(scope, response_json_key.into(), response_json.into());
    let response_body_key = v8::String::new_external_onebyte_static(scope, b"body").unwrap();
    let store = v8::ArrayBuffer::new_backing_store_from_vec(response.1);
    let response_body = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
    value.set(scope, response_body_key.into(), response_body.into());

    resolver.resolve(scope, value.into());
    // SAFETY: This is populated as long as the response is populated
    let dispose = unsafe { worker.should_dispose.assume_init() };
    dispose.send(()).map_err(|_| {
      anyhow!(
        "could not send dispoal command\nan isolate could not be cleaned up"
      )
    })?;
  }

  Ok(())
}
