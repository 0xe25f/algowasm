# AlgoWASM

AlgoWASM is a WebAssembly-first deterministic algorithmic music engine for modern browsers.

It includes:

- `dist/index.js`: the SDK as one minified ES module.
- `dist/index.d.ts`: public TypeScript declarations.
- `dist/algo_wasm.wasm`: Rust engine compiled to WebAssembly.
- `dist/worklet/algo-worklet.js`: the AudioWorklet processor as one self-contained minified file.
- `dist/profile-schema.json`: JSON schema for profiles, generated from the validator's own field table.

The package has no runtime dependencies.

See the [getting started guide](https://github.com/0xe25f/algowasm/blob/main/docs/getting-started.md) for integration steps, and the [live demo](https://0xe25f.github.io/algowasm/) to hear it.

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
