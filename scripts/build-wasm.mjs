import { copyFileSync, mkdirSync } from 'node:fs';
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

const result = spawnSync(
  'cargo',
  ['build', '-p', 'algo-wasm-ffi', '--target', 'wasm32-unknown-unknown', '--release'],
  {
    stdio: 'inherit',
    shell: false,
    env
  }
);

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const source = path.resolve('target/wasm32-unknown-unknown/release/algo_wasm_ffi.wasm');
const packageDir = path.resolve('packages/algo-wasm/pkg');
const target = path.join(packageDir, 'algo_wasm_ffi_bg.wasm');

mkdirSync(packageDir, { recursive: true });
copyFileSync(source, target);
console.log(`Built ${target}`);
