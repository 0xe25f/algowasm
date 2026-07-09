# AlgoWASM

AlgoWASM is a WebAssembly-first deterministic algorithmic music engine for modern browsers.

It includes:

- `dist/index.js`: ESM TypeScript SDK output.
- `dist/index.d.ts`: public TypeScript declarations.
- `dist/algo_wasm.wasm`: Rust engine compiled to WebAssembly.
- `dist/worklet/algo-worklet.js`: AudioWorklet processor.
- `dist/profile-schema.json`: JSON schema for profiles.

See the repository README for full integration notes, examples, browser deployment notes, and development commands.

## Quick Start

```ts
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

const player = await AlgoWasmPlayer.create({
  wasmUrl: '/algo_wasm.wasm',
  workletUrl: '/algo-worklet.js',
  profile: builtInProfiles.amigaHouse95ish(),
  seed: 42n
});

await player.start();
```

Call `start()` from a user gesture such as a click or tap.

## Licence

AGPL-3.0-or-later.

Any application that uses this library and is distributed or offered as a network service must make its complete source code available under the same licence.
