use deno_fetch::FetchPermissions;
use ::deno_web::{BlobStore, TimersPermission};
use anyhow::anyhow;
use deno_console::deno_console;
use deno_url::deno_url;
use deno_web::deno_web;
use deno_webidl::deno_webidl;

use deno_core::{
  error::JsError,
  serde_json::{map::ValuesMut, value},
  v8,
  v8::{
    undefined, Data, Function, FunctionCallback, Global, Local, Platform,
    Promise, PromiseState, SharedRef, Value,
  },
  FsModuleLoader, JsRuntime, ModuleSpecifier, RuntimeOptions,
};
use std::{
  f32::consts::E, future::poll_fn, path::Path, rc::Rc, sync::Arc,
  task::Context, task::Poll,
};

struct Permissions {}

impl TimersPermission for Permissions {
  fn allow_hrtime(&mut self) -> bool {
    true
  }

  fn check_unstable(&self, state: &deno_core::OpState, api_name: &'static str) {
    return;
  }
}

impl FetchPermissions for Permissions {
    fn check_net_url(
        &mut self,
        _url: &deno_core::url::Url,
        api_name: &str,
      ) -> Result<(), deno_core::error::AnyError> {
        Ok(())
    }

    fn check_read(&mut self, _p: &Path, api_name: &str) -> Result<(), deno_core::error::AnyError> {
      Ok(())
    }
}

pub struct SandboxRuntime {
  pub js_runtime: JsRuntime,
  root_mod_id: usize,
}

impl SandboxRuntime {
  pub fn new(platform: SharedRef<Platform>) -> Self {
    let js_runtime = JsRuntime::new(RuntimeOptions {
      // module_loader: (),
      compiled_wasm_module_store: None,
      inspector: false,
      is_main: true,
      module_loader: Some(Rc::new(FsModuleLoader {})), // TODO: create a module loader
      extensions: vec![
        deno_webidl::init_ops_and_esm(),
        deno_console::init_ops_and_esm(),
        deno_url::init_ops_and_esm(),
        deno_web::init_ops_and_esm::<Permissions>(Default::default(), None),
        deno_fetch::deno_fetch::init_ops_and_esm::<Permissions>(
          deno_fetch::Options {
          user_agent: "nCino Hypervisor / 0.1".to_owned(),
          root_cert_store_provider: None,
          unsafely_ignore_certificate_errors: None,
          file_fetch_handler: Rc::new(deno_fetch::FsFetchHandler),
          ..Default::default()
        }
        ),
        crate::ops::runtime::init_ops_and_esm(),
      ],
      v8_platform: Some(platform),
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
      .load_side_module(
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
    request: (String, Vec<u8>),
  // ) -> anyhow::Result<(String, Vec<u8>)> {
  ) -> anyhow::Result<v8::Global<v8::Value>> {
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

      let request_json = v8::String::new(scope, request.0.as_str()).ok_or_else(|| {anyhow!("could not convert request json to a value in the new isolate")})?;

      let backing_store = v8::ArrayBuffer::new_backing_store_from_vec(request.1);
      let request_body = v8::ArrayBuffer::with_backing_store(scope, &backing_store.make_shared());

      let maybe_response =
        func.call(scope, this, &[request_json.into(), request_body.into()]).ok_or_else(|| {
          anyhow!("handler function must return a response value")
        })?;

      let maybe_response = Global::new(scope, maybe_response);
      maybe_response
    };

    self.js_runtime.run_event_loop(false).await?;

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
  ) -> anyhow::Result<v8::Global<v8::Value>> {
    return poll_fn(|cx| self.poll_value(promise, cx)).await;
  }

  fn poll_value(
    &mut self,
    global: &Global<Value>,
    cx: &mut Context,
  ) -> Poll<Result<v8::Global<v8::Value>, anyhow::Error>> {
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
        },
        PromiseState::Rejected => {
          let result = promise.result(&mut scope);
          let err = JsError::from_v8_exception(&mut scope, result);
          Poll::Ready(Err(anyhow!(err)))
        }
      }
    } else {
      let value_handle = Global::new(&mut scope, local);
      return Poll::Ready(Ok(value_handle));
    }
  }
}
