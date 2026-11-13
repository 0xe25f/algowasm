// Builds the publishable package: minified JavaScript bundles, TypeScript
// declarations, the generated profile schema, and the WASM binary.
import { copyFile, mkdir, rm, stat, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { build } from 'vite';

const packageRoot = path.resolve('packages/algo-wasm');
const sourceRoot = path.join(packageRoot, 'src');
const distRoot = path.join(packageRoot, 'dist');

// Bundles entry points into minified ES modules with comments removed.
// `write: false` returns the code instead of writing it, for the schema step.
async function bundle(entry, { write = true } = {}) {
  const result = await build({
    configFile: false,
    logLevel: 'warn',
    build: {
      write,
      outDir: distRoot,
      emptyOutDir: false,
      target: 'es2022',
      sourcemap: false,
      lib: { entry, formats: ['es'] },
      rolldownOptions: { output: { minify: true, comments: false } }
    }
  });

  return (Array.isArray(result) ? result[0] : result).output;
}

// Start from an empty directory so removed files never linger in a release.
await rm(distRoot, { recursive: true, force: true });
await mkdir(distRoot, { recursive: true });

// Type-check and emit declarations only. The JavaScript comes from the bundler.
const tsc = spawnSync('npx', ['tsc', '-p', path.join(packageRoot, 'tsconfig.json')], {
  stdio: 'inherit',
  shell: false
});

if (tsc.status !== 0) {
  process.exit(tsc.status ?? 1);
}

await bundle({
  index: path.join(sourceRoot, 'index.ts'),
  'integrations/phaser': path.join(sourceRoot, 'integrations/phaser.ts'),
  'integrations/react': path.join(sourceRoot, 'integrations/react.ts'),
  'integrations/svelte': path.join(sourceRoot, 'integrations/svelte.ts'),
  'integrations/vue': path.join(sourceRoot, 'integrations/vue.ts')
});

// The worklet is bundled on its own into a single self-contained file.
// Bundlers treat a worklet URL as a plain asset and do not follow its imports.
await bundle({ 'worklet/algo-worklet': path.join(sourceRoot, 'worklet/algo-worklet.ts') });

// The schema is generated from the same field table the validator reads, so
// the published limits always match the ones enforced at runtime.
const [specChunk] = await bundle({ spec: path.join(sourceRoot, 'profile-spec.ts') }, { write: false });
const specModule = await import(
  `data:text/javascript;base64,${Buffer.from(specChunk.code).toString('base64')}`
);
await writeFile(
  path.join(distRoot, 'profile-schema.json'),
  `${JSON.stringify(specModule.buildProfileSchema(), null, 2)}\n`
);

const pkgWasm = path.join(packageRoot, 'pkg/algo_wasm_ffi_bg.wasm');
const pkgStat = await stat(pkgWasm).catch(() => null);

if (!pkgStat?.isFile()) {
  console.error('WASM binary not found. Run npm run build:wasm first.');
  process.exit(1);
}

await copyFile(pkgWasm, path.join(distRoot, 'algo_wasm.wasm'));
console.log('Built package JavaScript, declarations, schema, and WASM.');
