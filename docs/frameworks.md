# Framework Integrations

AlgoWASM has no framework dependency. The package ships small optional helpers for React, Vue, Svelte, and Phaser, shown below.

For LittleJS and three.js, see the [game guide](https://0xe25f.github.io/algowasm/games.html).

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
