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

`crates/algo-wasm-ffi` exposes a small C-compatible boundary. It owns raw pointer conversion, audio buffers, and error messages, and returns explicit error codes.

Neither crate has any dependencies. Everything crosses the boundary as numbers or buffers of numbers:

- A profile is a flat `Float32Array` in a fixed, versioned layout. `encodeProfile` in `packages/algo-wasm/src/profile-wire.ts` writes it, and `Profile::from_wire` reads it.
- A snapshot and the event list are `Float64Array` buffers, decoded by `packages/algo-wasm/src/wire-decode.ts`.
- Moods, tracks, wave shapes, scales, and sections are small integers in the order of the `WIRE_*` tables.

A panic inside the engine is a WASM trap. The binding treats a trap as fatal for that instance: it refuses every later call, and the player reports an `error` event.

## TypeScript package

`packages/algo-wasm` loads WASM, configures AudioWorklet, exposes `AlgoWasmPlayer`, validates profiles, and provides optional integration helpers.

The worklet instantiates the same WASM module and owns the real-time engine instance. The main thread sends commands. The worklet renders stereo samples into Web Audio output buffers.

`EngineCore` in `packages/algo-wasm/src/engine-core.ts` is the one binding to the WASM exports. The worklet and the main thread both use it. The build bundles it into the worklet file, because bundlers treat a worklet URL as a plain asset and do not follow its imports.

The main thread validates everything and turns it into numbers before sending it. The audio thread never parses text and never builds objects for events: it posts the raw buffers, and the main thread decodes them.

## Data flow

1. The frontend calls `AlgoWasmPlayer.create()`.
2. The SDK validates the profile and loads the worklet.
3. The SDK fetches `algo_wasm.wasm` and transfers its bytes to the worklet, which instantiates it.
4. The worklet creates a Rust engine from the sample rate, the seed, and the encoded profile.
5. The frontend calls `start()` after a user gesture.
6. The worklet calls the WASM render function for each audio block.
7. About every 21 ms the worklet posts the events buffered since its last post, with a snapshot. The engine keeps events until they are read, so none are lost.

## Memory ownership

The TypeScript layer allocates WASM buffers for profiles and render blocks through `algo_wasm_alloc()`. Every allocation is returned through `algo_wasm_dealloc()`. The render buffer is allocated once and reused.

Snapshot, event, and MIDI buffers live beside the engine and are freed with it. JavaScript copies out of them and never frees them.

The engine pointer is created by `algo_wasm_engine_create()` and destroyed by `algo_wasm_engine_destroy()`. `AlgoWasmPlayer.destroy()` and `AlgoWasmEngineBinding.destroy()` do this for you.

## Signal path

Each voice is rendered, panned, and summed into the dry mix. Each voice also feeds a delay bus and a reverb bus by its own send amounts. The dry mix and the two effect returns pass through a DC blocker and a look-ahead limiter, and master volume is applied last.
