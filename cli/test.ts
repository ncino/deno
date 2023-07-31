await Deno.runEdgeFunction(
  "../../hypervisor/test-data/hello-world.ts",
  new Request("https://localhost")
);
