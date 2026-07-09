import { copyFile, mkdir, readdir, stat } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';

const packageRoot = path.resolve('packages/algo-wasm');
const distRoot = path.join(packageRoot, 'dist');

async function* walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      yield* walk(fullPath);
      continue;
    }

    yield fullPath;
  }
}

const tsc = spawnSync('npx', ['tsc', '-p', path.join(packageRoot, 'tsconfig.json')], {
  stdio: 'inherit',
  shell: false
});

if (tsc.status !== 0) {
  process.exit(tsc.status ?? 1);
}

await mkdir(distRoot, { recursive: true });
await copyFile(path.join(packageRoot, 'src/profile-schema.json'), path.join(distRoot, 'profile-schema.json'));

const pkgDir = path.join(packageRoot, 'pkg');
const pkgWasm = path.join(pkgDir, 'algo_wasm_ffi_bg.wasm');
const pkgStat = await stat(pkgWasm).catch(() => null);

if (pkgStat?.isFile()) {
  await copyFile(pkgWasm, path.join(distRoot, 'algo_wasm.wasm'));
}

for await (const jsPath of walk(distRoot)) {
  if (!jsPath.endsWith('.js')) {
    continue;
  }

  const result = spawnSync(process.execPath, ['--check', jsPath], {
    stdio: 'inherit',
    shell: false
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

console.log('Built package JavaScript and declarations.');
