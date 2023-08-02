import { delay } from 'https://deno.land/x/delay@v0.2.0/mod.ts';

let promise = await Deno.runEdgeFunction(
  "/Users/tristanzander/Repos/hypervisor/hypervisor/test-data/hello-world.ts",
  new Request("https://localhost")
);

console.debug(promise)
