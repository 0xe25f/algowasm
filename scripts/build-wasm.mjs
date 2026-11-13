// Builds the release WASM binary and shrinks it with wasm-opt.
import { mkdirSync, rmSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';

const rustc = spawnSync('rustup', ['which', 'rustc'], {
  encoding: 'utf8',
  shell: false
});
const rustupBin =
  rustc.status === 0 && rustc.stdout.trim() !== ''
    ? path.dirname(rustc.stdout.trim())
    : undefined;
const env = {
  ...process.env,
  PATH: rustupBin ? `${rustupBin}${path.delimiter}${process.env.PATH ?? ''}` : process.env.PATH
};

function run(command, args) {
  const result = spawnSync(command, args, { stdio: 'inherit', shell: false, env });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const source = path.resolve('target/wasm32-unknown-unknown/release/algo_wasm_ffi.wasm');
const packageDir = path.resolve('packages/algo-wasm/pkg');
const target = path.join(packageDir, 'algo_wasm_ffi_bg.wasm');

// Remove the previous artefact first, so a failed build can never leave a
// stale binary behind to be packaged by mistake.
rmSync(packageDir, { recursive: true, force: true });
mkdirSync(packageDir, { recursive: true });

// Size-focused settings live in the workspace's [profile.release].
run('cargo', ['build', '-p', 'algo-wasm-ffi', '--target', 'wasm32-unknown-unknown', '--release']);

// wasm-opt comes from the binaryen development dependency. -Os keeps the
// audio loops fast, and the strip flags remove metadata the engine never
// reads. The enabled features are the ones current Rust emits for this
// target; every browser from 2025 onwards supports them.
run(path.resolve('node_modules/.bin/wasm-opt'), [
  source,
  '-Os',
  '--enable-bulk-memory',
  '--enable-mutable-globals',
  '--enable-nontrapping-float-to-int',
  '--enable-sign-ext',
  '--strip-debug',
  '--strip-producers',
  '--strip-target-features',
  '-o',
  target
]);

console.log(`Built ${target} (${statSync(target).size} bytes)`);
