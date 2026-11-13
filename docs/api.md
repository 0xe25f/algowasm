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

If loading fails, `create()` disconnects its audio node and closes any `AudioContext` it opened before it rejects. Nothing is left running.

## Transport

- `start()`: resumes the audio context and starts playback.
- `pause()`: pauses playback without resetting transport.
- `resume()`: resumes playback.
- `stop()`: stops playback and resets the engine.
- `destroy()`: disconnects audio, frees the engine's WASM memory, stops the worklet processor, and closes an owned `AudioContext`.

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

Each seed has its own tempo inside the profile's tempo range. `setTempoRange` keeps the song at the same relative place in the new range, so a seed that plays near the top of the profile range also plays near the top of the new range. Read the current tempo from the snapshot's `bpm`.

Track ids are `kick`, `snare`, `closed_hat`, `open_hat`, `percussion`, `bass`, `chords`, `pad`, `lead`, `arp`, and `fx`.

Moods are `dark`, `bright`, `tense`, and `calm`.

`setSeed()` and `setProfile()` stop playback and rewind the song. Call `start()` again to hear the result.

Invalid arguments throw straight away:

- `setTempoRange()` throws a `TypeError` for values that are not finite numbers, and a `RangeError` when `minBpm` is greater than `maxBpm`.
- `setMood()`, `setMuted()`, and `setSolo()` throw a `TypeError` for an unknown mood or track id.

A tempo range outside the profile's `limits` is reported through the `error` event, and the tempo stays as it was.

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

`getCurrentEvents()` returns the most recent batch of generated events from the worklet.

## Export

`exportMidi()` returns a `Uint8Array` containing a type-0 MIDI file of the current four-bar phrase. Pass `{ downloadName: 'phrase.mid' }` to also offer the file to the user as a download.

`renderOffline({ seconds, sampleRate, seed, profile })` returns interleaved stereo `Float32Array` data.

- `seconds` must be from 0 to 600. `sampleRate` must be a whole number from 22050 to 96000. Anything else throws a `RangeError`.
- The render uses the player's current volume, energy, intensity, mood, tempo range, mutes, and solos.
- The render runs in chunks and yields to the page between them, so the tab stays responsive.
- The WASM module is compiled on the first call and reused afterwards.

## Events

Use `on()` and `off()`:

```ts
const dispose = player.on('section-change', event => {
  console.log(event.section);
});

dispose();
```

Events are `ready`, `started`, `paused`, `resumed`, `stopped`, `section-change`, `bar`, `beat`, `error`, `warning`, and `metrics`.

Every `beat`, `bar`, and `section-change` is delivered. They arrive in batches about every 21 ms, so a handler can run up to that long after the audio. Use each event's `sample` field when you need exact timing.

## Engine errors

If the engine hits an internal error, the player emits an `error` event, playback stops, and every later command is rejected. Destroy the player and create a new one.

## Low-level binding

`AlgoWasmEngineBinding` drives one engine directly, without an `AudioContext`. It owns the WASM buffers it allocates, so call `destroy()` when finished.

```ts
const { instance } = await instantiateAlgoWasm('/algo_wasm.wasm');
const binding = new AlgoWasmEngineBinding(instance);

try {
  binding.create(48000, 42n, builtInProfiles.dubstep());
  binding.start();
  binding.renderTo(output);
} finally {
  binding.destroy();
}
```

`events()` returns the events generated since its last call and removes them from the engine's buffer. The buffer holds up to `limits.maxEventsPerBlock` events, so read it at least that often.
