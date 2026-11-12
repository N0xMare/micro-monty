// Runs a wasm32-wasip1 program on V8 through node:wasi, passing the environment.
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';
const wasi = new WASI({ version: 'preview1', args: [], env: process.env, preopens: {} });
const mod = await WebAssembly.compile(await readFile(process.argv[2]));
const inst = await WebAssembly.instantiate(mod, wasi.getImportObject());
wasi.start(inst);
