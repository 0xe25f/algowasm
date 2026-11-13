import { copyFile, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';

// The GitHub Pages demo has no bundler, so it cannot resolve the bare
// `@algo-wasm/algo-wasm` specifier the way the vite-based examples do.
// This script vendors the built package files straight from
// packages/algo-wasm/dist into demo/vendor/algo-wasm.
const repoRoot = path.resolve(import.meta.dirname, '..');
const distRoot = path.join(repoRoot, 'packages/algo-wasm/dist');
const vendorRoot = path.join(repoRoot, 'demo/vendor/algo-wasm');
const files = ['index.js', 'worklet/algo-worklet.js', 'algo_wasm.wasm'];

await rm(vendorRoot, { recursive: true, force: true });
await mkdir(path.join(vendorRoot, 'worklet'), { recursive: true });

for (const file of files) {
  await copyFile(path.join(distRoot, file), path.join(vendorRoot, file));
}

console.log('Vendored AlgoWASM assets for the static GitHub Pages demo.');
