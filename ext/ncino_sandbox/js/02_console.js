import { Console } from "ext:deno_console/01_console.js";


globalThis.console = new Console((msg, level) => Deno.core.ops.op_print(msg, level));

