# Getting Started

This page takes you from installing AlgoWASM to playing and controlling music in a page.

- Using a framework? See [frameworks.md](frameworks.md).
- Making a game? See the [game guide](https://0xe25f.github.io/algowasm/games.html) for LittleJS and three.js.
- Deploying? See [browser.md](browser.md) for MIME types and Content Security Policy.

## What It Does

- Generates coherent endless music in bars and sections.
- Uses deterministic seeded randomness.
- Provides four built-in style profiles. See [profiles.md](profiles.md).
- Renders stereo `f32` audio in WASM.
- Uses AudioWorklet for real-time playback.
- Exposes a small frontend-neutral TypeScript API.
- Supports live energy, intensity, mood, tempo, mute, and solo controls.
- Exports a compact MIDI file for the current generated phrase.
- Renders offline audio without an AudioWorklet.

## What It Does Not Do

- It does not stream music from a server.
- It does not use cloud AI.
- It does not bundle copyrighted samples.
- It does not clone AlgoMusic or SoundHelix code or assets.
- It does not support ScriptProcessorNode or legacy browsers.
- It does not require React, Vue, Svelte, Phaser, or any other framework.

## Browser Target

AlgoWASM targets current evergreen browsers from 2025 onwards. It assumes native ES modules, WebAssembly, AudioWorklet, `BigInt`, `TextEncoder`, `TextDecoder`, typed arrays, and standard Web Audio behaviour.

## Install

```sh
npm install @algo-wasm/algo-wasm
```

## Copying WASM and Worklet Assets

Copy these package files to your public asset directory:

```text
node_modules/@algo-wasm/algo-wasm/dist/algo_wasm.wasm
node_modules/@algo-wasm/algo-wasm/dist/worklet/algo-worklet.js
```

For Vite, place them in `public/`:

```text
public/algo_wasm.wasm
public/algo-worklet.js
```

Then pass those URLs to `AlgoWasmPlayer.create()`.

## Quick Start

```ts
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

const button = document.querySelector<HTMLButtonElement>('#start');

button?.addEventListener('click', async () => {
  const player = await AlgoWasmPlayer.create({
    wasmUrl: '/algo_wasm.wasm',
    workletUrl: '/algo-worklet.js',
    profile: builtInProfiles.amigaHouse95ish(),
    seed: 42n
  });

  await player.start();
});
```

Call `start()` from a user gesture such as a click or tap. Browsers can block audio if playback starts from page load.

## Vanilla JS Integration

```ts
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

let player: AlgoWasmPlayer | undefined;
let playerPromise: Promise<AlgoWasmPlayer> | undefined;

async function startMusic() {
  // Memoise the in-flight promise, not just the eventual player, so an
  // overlapping call such as a fast double click awaits the same player
  // instead of creating a second AudioContext and engine.
  playerPromise ??= AlgoWasmPlayer.create({
    wasmUrl: '/algo_wasm.wasm',
    workletUrl: '/algo-worklet.js',
    profile: builtInProfiles.amigaHouse95ish(),
    seed: 1995n,
    volume: 0.8
  });

  player = await playerPromise;
  await player.start();
}
```

The repository includes `examples/vanilla`. It lets users enter a seed, choose a track length in `MM:SS`, switch between built-in profiles, and start a new generated track with a random seed.

## Custom Frontend Integration

The main package has no framework dependency. Use `AlgoWasmPlayer` from any frontend that can call browser APIs. Keep asset URLs explicit when your bundler does not copy `.wasm` and worklet files automatically.

## User Gesture and Audio Unlock

Create or start playback from a user gesture. `AlgoWasmPlayer.create()` may create an `AudioContext`, but `start()` calls `audioContext.resume()` and can fail if the browser still considers the context locked.

## Seeds and Deterministic Generation

The same profile, seed, engine version, and sample rate produce the same event stream. Use `bigint`, safe integer numbers, or decimal strings:

```ts
await AlgoWasmPlayer.create({
  wasmUrl: '/algo_wasm.wasm',
  workletUrl: '/algo-worklet.js',
  seed: 123456789n
});
```

## Live Controls

```ts
player.setEnergy(0.7);
player.setIntensity(0.5);
player.setMood('dark');
player.setTempoRange(122, 132);
player.setMuted('lead', true);
player.setSolo('bass', false);
```

Live numeric controls are clamped to safe ranges. Profile changes are validated. An unknown mood or track id, or a tempo range that is not finite and ordered, throws straight away.

## Offline Rendering

```ts
const audio = await player.renderOffline({
  seconds: 8,
  sampleRate: 48000,
  seed: 99n
});
```

The result is interleaved stereo `Float32Array` data. The render uses the player's current volume, energy, intensity, mood, mutes, and solos, accepts up to 600 seconds, and yields to the page as it runs.

## MIDI Export

```ts
const midi = await player.exportMidi();
const blob = new Blob([midi], { type: 'audio/midi' });
```

The exporter writes the current generated phrase as a compact type-0 MIDI file. Pass `{ downloadName: 'phrase.mid' }` to also offer it as a download.

## Troubleshooting

- `AudioWorklet is not available`: use a current evergreen browser.
- `AudioContext could not resume`: call `start()` from a click or tap.
- `WASM streaming compilation failed`: check the `.wasm` MIME type.
- `Profile must include one patch for each built-in track`: keep all 11 track patches.
- Silence after start: confirm the worklet and WASM URLs are correct and same-origin.
