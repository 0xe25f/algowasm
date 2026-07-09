# Architecture

AlgoWASM is split into a Rust core, a raw WASM FFI crate, and a TypeScript browser SDK.

```text
Frontend app
  |
  v
@algo-wasm/algo-wasm
  |
  v
AudioContext + AudioWorkletNode
  |
  v
algo-worklet.js
  |
  v
algo_wasm.wasm
  |
  v
Rust engine, synth, mixer, and DSP
```

## Rust crates

`crates/algo-wasm-core` contains deterministic composition, profile validation, synthesis, rendering, offline output, and MIDI export. It has no browser APIs.

`crates/algo-wasm-ffi` exposes a small C-compatible boundary. It owns raw pointer conversion, JSON buffers, audio buffers, and error messages. It catches panics at the boundary and returns explicit error codes.

## TypeScript package

`packages/algo-wasm` loads WASM, configures AudioWorklet, exposes `AlgoWasmPlayer`, validates profiles, and provides optional integration helpers.

The worklet instantiates the same WASM module and owns the real-time engine instance. The main thread sends commands. The worklet renders interleaved stereo samples into Web Audio output buffers.

## Data flow

1. The frontend calls `AlgoWasmPlayer.create()`.
2. The SDK validates the profile and loads the worklet.
3. The worklet fetches and instantiates `algo_wasm.wasm`.
4. The worklet creates a Rust engine from sample rate, seed, and profile JSON.
5. The frontend calls `start()` after a user gesture.
6. The worklet calls the WASM render function for each audio block.
7. Events and snapshots flow back to the main thread outside the hot render path.

## Memory ownership

The TypeScript layer allocates temporary WASM buffers for strings and render blocks through `algo_wasm_alloc()`. Every temporary allocation is returned through `algo_wasm_dealloc()`.

The engine pointer is created by `algo_wasm_engine_create()` and destroyed by `algo_wasm_engine_destroy()`.
