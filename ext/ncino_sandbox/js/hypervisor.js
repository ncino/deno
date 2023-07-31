const Deno = globalThis.Deno;
const core = Deno.core;
const ops = core.ops;

export function runEdgeFunction(file, request) {
  return ops.op_run_edge_function(file, request);
};

