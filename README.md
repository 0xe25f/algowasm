# AlgoWASM

[![npm version](https://img.shields.io/npm/v/%40algo-wasm%2Falgo-wasm.svg)](https://www.npmjs.com/package/@algo-wasm/algo-wasm)
[![CI](https://github.com/0xe25f/algowasm/actions/workflows/ci.yml/badge.svg)](https://github.com/0xe25f/algowasm/actions/workflows/ci.yml)
[![Licence: AGPL-3.0-or-later](https://img.shields.io/badge/licence-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![GitHub stars](https://img.shields.io/github/stars/0xe25f/algowasm?style=social)](https://github.com/0xe25f/algowasm/stargazers)

AlgoWASM is a WebAssembly-first algorithmic music library for modern browsers.

> **What this does, in plain words:** AlgoWASM makes up new music by itself, right inside your web browser. You give it a starting number, called a seed. It uses that number to write a song and play it straight away. It does not play a music file you already have. It invents the beat, the bass, and the tune as it goes. The same seed always makes the same song again.

It creates endless electronic music locally in the browser from a seed and a style profile. The composition engine, procedural synthesis, scheduling, DSP, offline rendering, and MIDI export live in Rust compiled to WebAssembly. The TypeScript package is a thin browser integration layer around loading, AudioWorklet playback, controls, and events.

If AlgoWASM is useful to you, please consider starring the [repository on GitHub](https://github.com/0xe25f/algowasm). It helps other people discover the project.

## What It Does

- Generates coherent endless music in bars and sections.
- Uses deterministic seeded randomness.
- Provides the built-in `amiga_house_95ish` profile.
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

## React Integration

```tsx
import { useEffect, useRef, useState } from 'react';
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

export function MusicButton() {
  const playerRef = useRef<AlgoWasmPlayer | null>(null);
  const playerPromiseRef = useRef<Promise<AlgoWasmPlayer> | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);

  useEffect(() => {
    return () => {
      void playerRef.current?.destroy();
    };
  }, []);

  async function toggleMusic() {
    // Memoise the in-flight promise so an overlapping call awaits the same
    // player instead of creating a second AudioContext and engine.
    playerPromiseRef.current ??= AlgoWasmPlayer.create({
      wasmUrl: '/algo_wasm.wasm',
      workletUrl: '/algo-worklet.js',
      profile: builtInProfiles.amigaHouse95ish(),
      seed: 1995n
    });

    playerRef.current = await playerPromiseRef.current;

    if (isPlaying) {
      await playerRef.current.pause();
      setIsPlaying(false);
      return;
    }

    await playerRef.current.start();
    setIsPlaying(true);
  }

  return (
    <button onClick={toggleMusic}>
      {isPlaying ? 'Pause Music' : 'Start Music'}
    </button>
  );
}
```

The repository includes `examples/react`. It shows the same seed, `MM:SS` length, profile selection, and random-track controls in a component frontend.

## Vue Integration

```ts
import { onBeforeUnmount, ref } from 'vue';
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

const player = ref<AlgoWasmPlayer>();
let playerPromise: Promise<AlgoWasmPlayer> | undefined;
const isPlaying = ref(false);

export async function toggleMusic() {
  // Memoise the in-flight promise so an overlapping call awaits the same
  // player instead of creating a second AudioContext and engine.
  playerPromise ??= AlgoWasmPlayer.create({
    wasmUrl: '/algo_wasm.wasm',
    workletUrl: '/algo-worklet.js',
    profile: builtInProfiles.amigaHouse95ish(),
    seed: 44n
  });

  player.value = await playerPromise;

  if (isPlaying.value) {
    await player.value.pause();
    isPlaying.value = false;
    return;
  }

  await player.value.start();
  isPlaying.value = true;
}

onBeforeUnmount(() => {
  void player.value?.destroy();
});
```

## Svelte Integration

```ts
import { onDestroy } from 'svelte';
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

let player: AlgoWasmPlayer | undefined;
let playerPromise: Promise<AlgoWasmPlayer> | undefined;

export async function startMusic() {
  // Memoise the in-flight promise so an overlapping call awaits the same
  // player instead of creating a second AudioContext and engine.
  playerPromise ??= AlgoWasmPlayer.create({
    wasmUrl: '/algo_wasm.wasm',
    workletUrl: '/algo-worklet.js',
    profile: builtInProfiles.amigaHouse95ish(),
    seed: 88n
  });

  player = await playerPromise;
  await player.start();
}

onDestroy(() => {
  void player?.destroy();
});
```

## Phaser Integration

```ts
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

export class BootScene extends Phaser.Scene {
  private music?: AlgoWasmPlayer;

  async create() {
    this.input.once('pointerdown', async () => {
      this.music = await AlgoWasmPlayer.create({
        wasmUrl: '/algo_wasm.wasm',
        workletUrl: '/algo-worklet.js',
        profile: builtInProfiles.amigaHouse95ish(),
        seed: 1234n
      });

      await this.music.start();
    });
  }

  update() {
    const danger = this.registry.get('danger') ?? 0;
    this.music?.setEnergy(Number(danger));
  }
}
```

The repository includes `examples/phaser`. It exposes seed, `MM:SS` length, profile selection, and random-track controls around the game canvas.

## Custom Frontend Integration

The main package has no framework dependency. Use `AlgoWasmPlayer` from any frontend that can call browser APIs. Keep asset URLs explicit when your bundler does not copy `.wasm` and worklet files automatically.

## User Gesture and Audio Unlock

Create or start playback from a user gesture. `AlgoWasmPlayer.create()` may create an `AudioContext`, but `start()` calls `audioContext.resume()` and can fail if the browser still considers the context locked.

## Serving `.wasm` Correctly

Serve `.wasm` files with:

```text
application/wasm
```

AlgoWASM uses `WebAssembly.instantiateStreaming()` when possible. If the server uses the wrong MIME type during local testing, the loader falls back to `fetch()`, `arrayBuffer()`, and `WebAssembly.instantiate()`.

## Content Security Policy Notes

Strict CSP can affect WebAssembly compilation. Check your browser console if loading fails. A typical deployment allows WebAssembly compilation and serves the worklet as a same-origin script.

## Profiles

Profiles are JSON-compatible style definitions. They include tempo, scales, chord rules, rhythm rules, bass rules, melody rules, arrangement rules, automation rules, patches, mixer settings, and safety limits.

### Built-In Profiles

- `builtInProfiles.amigaHouse95ish()`: the default mid-1990s Amiga-inspired house profile, 118 to 138 BPM.
- `builtInProfiles.downtempoBreakbeat()`: a slower, broken-beat profile, 88 to 112 BPM.
- `builtInProfiles.dubDeepHouse()`: a roomier, more hypnotic dub and deep house profile, 118 to 124 BPM.

```ts
import { builtInProfiles, validateProfile } from '@algo-wasm/algo-wasm';

const profile = builtInProfiles.amigaHouse95ish();
profile.tempo.minBpm = 122;
profile.tempo.maxBpm = 132;

validateProfile(profile);
```

Invalid profiles fail before they reach WASM. Error messages use British English and explain the invalid field.

See [docs/profiles.md](docs/profiles.md) for the full field reference, sound-design notes, and a guide to building custom profiles.

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

Live numeric controls are clamped to safe ranges. Profile changes are validated.

## Offline Rendering

```ts
const audio = await player.renderOffline({
  seconds: 8,
  sampleRate: 48000,
  seed: 99n
});
```

The result is interleaved stereo `Float32Array` data.

## MIDI Export

```ts
const midi = await player.exportMidi();
const blob = new Blob([midi], { type: 'audio/midi' });
```

The exporter writes the current generated phrase as a compact type-0 MIDI file.

## Performance Notes

- Real-time rendering uses a fixed voice pool.
- The AudioWorklet render path avoids JSON parsing and logging.
- The first version does not require `SharedArrayBuffer`.
- The WASM binary size is tracked by the build output.
- The engine clamps gains, feedback, tempo, and sample rates.

## Troubleshooting

- `AudioWorklet is not available`: use a current evergreen browser.
- `AudioContext could not resume`: call `start()` from a click or tap.
- `WASM streaming compilation failed`: check the `.wasm` MIME type.
- `Profile must include one patch for each built-in track`: keep all 11 track patches.
- Silence after start: confirm the worklet and WASM URLs are correct and same-origin.

## API Reference

```ts
const player = await AlgoWasmPlayer.create(options);
await player.start();
await player.pause();
await player.resume();
await player.stop();
await player.destroy();

player.setSeed(123n);
player.setProfile(profile);
player.setVolume(0.8);
player.setMuted('lead', true);
player.setSolo('bass', false);
player.setEnergy(0.7);
player.setIntensity(0.5);
player.setMood('dark');
player.setTempoRange(122, 132);

const snapshot = player.getSnapshot();
const events = player.getCurrentEvents();
const midi = await player.exportMidi();
const audio = await player.renderOffline({ seconds: 4 });
```

Events:

- `ready`
- `started`
- `paused`
- `resumed`
- `stopped`
- `section-change`
- `bar`
- `beat`
- `error`
- `warning`
- `metrics`

See [docs/api.md](docs/api.md) for more detail.

## Development

```sh
npm install
npm run build
cargo test --workspace
npm test
```

Build scripts:

- `npm run build:wasm` builds `crates/algo-wasm-ffi` for `wasm32-unknown-unknown`.
- `npm run build:ts` compiles the TypeScript package and copies assets.
- `node scripts/copy-example-assets.mjs` copies WASM and worklet files into example `public/` folders.

## Testing

```sh
cargo test --workspace
npm test
npm run test:browser
```

Browser tests use Playwright and run from an HTTP server so fetch, MIME types, modules, and AudioWorklet behave like production.

## Release

1. Run all tests.
2. Run `npm run build`.
3. Check `packages/algo-wasm/dist`.
4. Confirm package contents include JavaScript, declarations, WASM, worklet, README, and licence.
5. Publish from `packages/algo-wasm`.

## Licence

AGPL-3.0-or-later. See [LICENSE](LICENSE).

Any application that uses this library and is distributed or offered as a network service must make its complete source code available under the same licence.
