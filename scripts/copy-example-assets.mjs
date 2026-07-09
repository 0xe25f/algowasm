import { copyFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const names = process.argv.slice(2);
const examples = names.length > 0 ? names : ['vanilla', 'react', 'phaser'];
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const distRoot = path.join(repoRoot, 'packages/algo-wasm/dist');

for (const name of examples) {
  const publicDir = path.join(repoRoot, 'examples', name, 'public');
  await mkdir(publicDir, { recursive: true });
  await copyFile(path.join(distRoot, 'algo_wasm.wasm'), path.join(publicDir, 'algo_wasm.wasm'));
  await copyFile(
    path.join(distRoot, 'worklet/algo-worklet.js'),
    path.join(publicDir, 'algo-worklet.js')
  );
  console.log(`Copied AlgoWASM assets for ${name}.`);
}
