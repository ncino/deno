import { RequestModel, ResponseModel } from 'ext:runtime/01_models.js'
import 'ext:runtime/02_console.js'
import 'ext:deno_webidl/00_webidl.js'
import 'ext:deno_url/00_url.js'
import 'ext:deno_url/01_urlpattern.js'
import 'ext:deno_web/00_infra.js'
import 'ext:deno_web/01_dom_exception.js'
import 'ext:deno_web/01_mimesniff.js'
import 'ext:deno_web/02_event.js'
import 'ext:deno_web/02_structured_clone.js'
import 'ext:deno_web/02_timers.js'
import 'ext:deno_web/03_abort_signal.js'
import 'ext:deno_web/04_global_interfaces.js'
import 'ext:deno_web/05_base64.js'
import 'ext:deno_web/06_streams.js'
import 'ext:deno_web/08_text_encoding.js'
import 'ext:deno_web/09_file.js'
import 'ext:deno_web/10_filereader.js'
import 'ext:deno_web/12_location.js'
import 'ext:deno_web/13_message_port.js'
import 'ext:deno_web/14_compression.js'
import 'ext:deno_web/15_performance.js'
import 'ext:deno_fetch/20_headers.js';
import 'ext:deno_fetch/21_formdata.js';
import 'ext:deno_fetch/22_body.js';
import 'ext:deno_fetch/22_http_client.js';
import 'ext:deno_fetch/23_request.js';
import { Response } from 'ext:deno_fetch/23_response.js';
import { Request } from 'ext:deno_fetch/23_request.js';
import 'ext:deno_fetch/26_fetch.js';

globalThis.Response = Response;
globalThis.Request = Request;
globalThis.RequestModel = RequestModel;
globalThis.ResponseModel = ResponseModel;
