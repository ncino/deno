const Deno = globalThis.Deno;
const core = Deno.core;
const ops = core.ops;

export async function runEdgeFunction(file, request) {
  let fut = ops.op_run_edge_function(file, request);
  console.log(fut);
  return await fut;
}

