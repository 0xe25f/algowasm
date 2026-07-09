import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

// The GitHub Pages demo has no bundler, so it cannot resolve the bare
// `@algo-wasm/algo-wasm` specifier the way the vite-based examples do.
// This script vendors the plain ES module build straight from
// packages/algo-wasm/dist into demo/vendor/algo-wasm, stripping the
// sourceMappingURL comments (the .map files themselves are not copied).
const repoRoot = path.resolve(import.meta.dirname, '..');
const distRoot = path.join(repoRoot, 'packages/algo-wasm/dist');
const vendorRoot = path.join(repoRoot, 'demo/vendor/algo-wasm');

const jsModules = [
  'index.js',
  'AlgoWasmError.js',
  'AlgoWasmPlayer.js',
  'events.js',
  'loadWasm.js',
  'profile.js'
];

async function copyModule(fileName) {
  const source = path.join(distRoot, fileName);
  const target = path.join(vendorRoot, fileName);
  const contents = await readFile(source, 'utf8');
  const withoutSourceMap = contents.replace(/^\/\/# sourceMappingURL=.*\n?$/m, '');
  await writeFile(target, withoutSourceMap);
}

await mkdir(vendorRoot, { recursive: true });
await mkdir(path.join(vendorRoot, 'worklet'), { recursive: true });

for (const fileName of jsModules) {
  await copyModule(fileName);
}

await copyFile(path.join(distRoot, 'algo_wasm.wasm'), path.join(vendorRoot, 'algo_wasm.wasm'));
await copyModule('worklet/algo-worklet.js');

console.log('Vendored AlgoWASM assets for the static GitHub Pages demo.');
