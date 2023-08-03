import { setTimeout, clearTimeout } from "ext:deno_web/02_timers.js";

const Deno = globalThis.Deno;
const core = Deno.core;
const ops = core.ops;

/**
 * Run an edge function with tenancy defined.
 * @param {string} file
 * @param {Request} request
 * @returns {Promise<Response>}
 */
export async function runEdgeFunction(file, requestModel, bodyArrayBuf) {
  const edgeFunctionPromise = ops.op_start_edge_function(file, JSON.stringify(requestModel), new Uint8Array(bodyArrayBuf));
  let reject;
  const timeoutPromise = new Promise((_res, rej) => {
    reject = rej;
  });

  const timeout = setTimeout(() => reject("edge function timed out"), 10 * 1000);

  const res = await Promise.race([edgeFunctionPromise, timeoutPromise]);

  if (timeout) {
    clearTimeout(timeout);
  }

  return res;
}

core.setMacrotaskCallback(handleWorkers);
function handleWorkers() {
  ops.op_drain_worker_queue();

  return undefined;
}
