const req = new Request("https://localhost");
let res = await Deno.runEdgeFunction(
  "/Users/tristanzander/Repos/hypervisor/hypervisor/test-data/hello-world.ts",
  new RequestModel(req),
  req.arrayBuffer()
);

console.debug("Finished", res);
