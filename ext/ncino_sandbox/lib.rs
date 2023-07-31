use anyhow::anyhow;
use deno_console::deno_console;
use deno_core::{
  error::JsError,
  v8::{undefined, Function, Global, Local, Promise, PromiseState, Value},
  FsModuleLoader, JsRuntime, ModuleSpecifier, RuntimeOptions,
};
use std::{future::poll_fn, path::Path, rc::Rc, task::Context, task::Poll};

pub mod ops;

pub struct SandboxWorker {
  pub js_runtime: JsRuntime,
  root_mod_id: usize,
}

impl SandboxWorker {
  pub fn new() -> Self {
    let js_runtime = JsRuntime::new(RuntimeOptions {
      // module_loader: (),
      compiled_wasm_module_store: None,
      inspector: false,
      is_main: true,
      module_loader: Some(Rc::new(FsModuleLoader {})), // TODO: create a module loader
      extensions: vec![
        deno_console::init_ops_and_esm(),
        ops::runtime::init_ops_and_esm(),
      ],
      ..Default::default()
    });

    Self {
      js_runtime,
      root_mod_id: 0,
    }
  }

  /// Load the main module and bootstrap the runtime.
  /// This must be run before calling Self::run() since it loads the module to be run.
  pub async fn bootstrap(&mut self, module_path: &Path) -> anyhow::Result<()> {
    let mod_id = self
      .js_runtime
      .load_main_module(
        &ModuleSpecifier::from_file_path(module_path)
          .map_err(|_| anyhow!("could not create url from module path"))?,
        None,
      )
      .await?;

    self.root_mod_id = mod_id;

    let rx = self.js_runtime.mod_evaluate(mod_id);

    self.js_runtime.run_event_loop(false).await?;
    rx.await??;

    #[cfg(debug_assertions)]
    {
      let module_ns = self.js_runtime.get_module_namespace(mod_id)?;
      let result = module_ns.open(self.js_runtime.v8_isolate());

      debug_assert!(
        result.is_module_namespace_object(),
        "did not return a module namespace object"
      );
    }

    Ok(())
  }

  /// Runs the sandbox until completion. Returns the response from the handler function, usually a Response object.
  pub async fn run(
    &mut self,
    request_obj: Local<'_, deno_core::v8::Value>,
  ) -> anyhow::Result<Global<Value>> {
    let module_ns = self.js_runtime.get_module_namespace(self.root_mod_id)?;
    let result = module_ns.open(self.js_runtime.v8_isolate());

    let handler = {
      let mut scope = self.js_runtime.handle_scope();
      let scope = &mut scope;
      let default_key = deno_core::v8::String::new(scope, "default")
        .expect("could not create string \"default\"")
        .into();
      let default_export = result
        .get(scope, default_key)
        .ok_or_else(|| anyhow!("main module does not have a default export"))?;

      if !default_export.is_async_function() && !default_export.is_function() {
        return Err(anyhow!("default export is not a handler function"));
      }

      // SAFETY: This is checked to be a function
      let handler = Global::new(scope, unsafe {
        Local::cast(default_export) as Local<'_, Function>
      });

      handler
    };

    let response = {
      let mut scope = self.js_runtime.handle_scope();
      let scope = &mut scope;
      let func = handler.open(scope);
      let this = undefined(scope).into();

      let maybe_response =
        func.call(scope, this, &[request_obj]).ok_or_else(|| {
          anyhow!("handler function must return a response value")
        })?;

      let maybe_response =
        Global::new(scope, unsafe { Local::cast(maybe_response) });
      maybe_response
    };

    let response = self.await_response(&response).await?;
    {
      let mut scope = self.js_runtime.handle_scope();
      let scope = &mut scope;
      let response = response.open(scope);

      println!(
        "Result Type: {}",
        response.type_of(scope).to_rust_string_lossy(scope)
      );
    }

    Ok(response)
  }

  async fn await_response(
    &mut self,
    promise: &Global<Value>,
  ) -> anyhow::Result<Global<Value>> {
    return poll_fn(|cx| self.poll_value(promise, cx)).await;
  }

  fn poll_value(
    &mut self,
    global: &Global<Value>,
    cx: &mut Context,
  ) -> Poll<Result<Global<Value>, anyhow::Error>> {
    let state = self.js_runtime.poll_event_loop(cx, false);

    let mut scope = self.js_runtime.handle_scope();
    let local = Local::<Value>::new(&mut scope, global);

    if let Ok(promise) = Local::<Promise>::try_from(local) {
      match promise.state() {
        PromiseState::Pending => match state {
          Poll::Ready(Ok(_)) => {
            let msg = "Promise resolution is still pending but the event loop has already resolved.";
            Poll::Ready(Err(anyhow!(msg)))
          }
          Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
          Poll::Pending => Poll::Pending,
        },
        PromiseState::Fulfilled => {
          let value = promise.result(&mut scope);
          let value_handle = Global::new(&mut scope, value);
          Poll::Ready(Ok(value_handle))
        }
        PromiseState::Rejected => {
          let result = promise.result(&mut scope);
          let err = JsError::from_v8_exception(&mut scope, result);
          Poll::Ready(Err(anyhow!(err)))
        }
      }
    } else {
      let value_handle = Global::new(&mut scope, local);
      Poll::Ready(Ok(value_handle))
    }
  }
}
