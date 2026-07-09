# API

## `AlgoWasmPlayer.create(options)`

Creates a player.

```ts
const player = await AlgoWasmPlayer.create({
  wasmUrl: '/algo_wasm.wasm',
  workletUrl: '/algo-worklet.js',
  profile: builtInProfiles.amigaHouse95ish(),
  seed: 42n,
  volume: 0.8
});
```

Options:

- `wasmUrl`: URL for `algo_wasm.wasm`.
- `workletUrl`: URL for `algo-worklet.js`.
- `profile`: optional style profile. Defaults to `builtInProfiles.amigaHouse95ish()`. See [profiles.md](profiles.md) for the full list of built-in profiles and the field reference.
- `seed`: `bigint`, safe integer, or decimal string.
- `volume`: master volume from 0 to 1.
- `audioContext`: optional caller-owned `AudioContext`.

## Transport

- `start()`: resumes the audio context and starts playback.
- `pause()`: pauses playback without resetting transport.
- `resume()`: resumes playback.
- `stop()`: stops playback and resets the engine.
- `destroy()`: disconnects audio, frees WASM state, and closes an owned `AudioContext`.

Repeated `start()`, `stop()`, and `destroy()` calls are safe.

## Live controls

- `setSeed(seed)`
- `setProfile(profile)`
- `setVolume(volume)`
- `setMuted(trackId, muted)`
- `setSolo(trackId, solo)`
- `setEnergy(value)`
- `setIntensity(value)`
- `setMood(mood)`
- `setTempoRange(minBpm, maxBpm)`

Track ids are `kick`, `snare`, `closed_hat`, `open_hat`, `percussion`, `bass`, `chords`, `pad`, `lead`, `arp`, and `fx`.

Moods are `dark`, `bright`, `tense`, and `calm`.

## State

`getSnapshot()` returns:

```ts
{
  seed: '42',
  sampleRate: 48000,
  bpm: 126,
  currentBar: 0,
  currentBeat: 1,
  currentSection: 'intro',
  energy: 0.5,
  intensity: 0.5,
  isPlaying: true,
  tracks: []
}
```

`getCurrentEvents()` returns recent generated events from the worklet.

## Export

`exportMidi()` returns a `Uint8Array` containing a type-0 MIDI file.

`renderOffline({ seconds, sampleRate, seed, profile })` returns interleaved stereo `Float32Array` data.

## Events

Use `on()` and `off()`:

```ts
const dispose = player.on('section-change', event => {
  console.log(event.section);
});

dispose();
```

Events are `ready`, `started`, `paused`, `resumed`, `stopped`, `section-change`, `bar`, `beat`, `error`, `warning`, and `metrics`.
