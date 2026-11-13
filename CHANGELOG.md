# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased] - planned as 1.2.0

### Added

- A `dubstep` built-in profile: half-time drums, a sustained sine sub bass, sparse mids, and a tempo-locked wobble.
- A stereo reverb, fed by each patch's `reverbSend`. Earlier releases sent `reverbSend` to the delay.
- Kick ducking, a second detuned oscillator, a filter envelope, a tempo-synced filter LFO, and stereo width, through the optional patch fields `duck`, `detuneCents`, `filterEnvOctaves`, `filterEnvDecayMs`, `lfoRate`, `lfoDepth`, and `width`.
- Optional `rhythmRules.halfTimeChance` and `bassRules.sustainChance`.
- `exportMidi({ downloadName })` now offers the file as a download. The option was accepted but ignored before.
- `profiles/*.json` as the single source for built-in profiles, with `npm run generate:profiles`.

### Changed

- Sound: the master waveshaper is replaced by a look-ahead limiter, the delay is tempo-synced, oscillators are band-limited, and the built-in profiles are rebalanced so chords, lead, and arp are audible. Every seed still composes the same notes as in 1.1.0, but renders are not sample-identical.
- The WASM binary is 103 KB, down from 347 KB. The JavaScript is bundled and minified, and source maps are no longer published.
- The engine has no Rust dependencies. `serde` and `serde_json` are removed, and profiles, snapshots, and events cross the WASM boundary as numeric buffers.
- `renderOffline` applies the player's live volume, energy, intensity, mood, tempo range, mutes, and solos, yields to the page while rendering, and reuses the compiled module.
- Master volume is applied after the limiter.
- `AlgoWasmEngineBinding.events()` returns the events since its last call, not only those of the last rendered block.
- `limits.maxVoices` is honoured. It was validated but ignored before.

### Fixed

- About 94% of `beat`, `bar`, and `section-change` events never reached JavaScript. Every event is now delivered.
- The delay send multiplied the whole mix by a level instead of sending each track's own signal, which distorted the mix.
- Notes clicked at release, on retrigger, and when a voice was reused. Release times longer than 80 ms were cut short.
- `AlgoWasmPlayer.create` left a connected node and an open `AudioContext` behind when loading failed.
- A destroyed player's worklet processor kept running inside a caller-owned `AudioContext`.
- `setTempoRange(NaN, NaN)` crashed the engine.
- A profile with a very large `limits.maxEventsPerBlock` could exhaust memory.
- Closed hi-hats now cut off a ringing open hi-hat.
- The one-pole filter used a wrong coefficient, so the effects sweep differed between sample rates.

### Migration

No exported name was removed or renamed. These inputs were accepted before and are now rejected:

- Unknown scale names. They used to fall back to natural minor without warning. Use `natural_minor`, `dorian`, `aeolian`, `phrygian_dominant`, or `minor_pentatonic`.
- Out-of-range `chordRules.cadenceWeight`, `chordRules.movementWeight` (0 to 4), `melodyRules.maxRangeSemitones` (1 to 48), `limits.maxEventsPerBlock` (1 to 512), envelope times above 10,000 ms, more than eight scales, and section lengths above 64 bars.
- `setMood`, `setMuted`, and `setSolo` throw a `TypeError` for an unknown id, and `setTempoRange` throws for values that are not finite and ordered. They used to report through the `error` event.
- `renderOffline` throws a `RangeError` for `seconds` outside 0 to 600 or an unsupported `sampleRate`.

The WASM file and the JavaScript must come from the same release. A mismatch is reported as an error when the engine is created.

The Rust crate `algo-wasm-core` is not published, but if you depend on it by path: `Profile::from_json`, `Profile::to_json`, `Engine::snapshot_json`, and `Engine::current_events_json` are removed. Use `Profile::from_wire`, `Snapshot::to_wire`, and `Engine::drain_events`.

## [1.1.0] - 2026-09-23

### Added

- A per-song genome generated from each seed, including tempo within the profile range, home key, groove with swing, lead motif, bassline shape, signature progressions, drum kit, patch variations, and automation.
- Voice-led chord construction from the chosen scale, with home-key continuity and occasional short related-key moves.
- Section-aware arrangement branching with A/B phrases, fill styles, layer fades, and energy-weighted seeded choices.
- Support for `chordRules`, `automationRules`, `mutationChance`, and `callResponseChance` where they were previously validated but ignored.
- A resonant state-variable filter so patch resonance is audible in the rendered output.
- Seed-diversity regression tests for song identity, scale correctness, and bounded renders.

### Changed

- Each seed now produces its own distinct musical identity instead of sharing the same tempo, groove grid, chord shapes, patches, and section order.
- The lead melody now follows a call-response-call-cadence pattern and stays within key, replacing the previous clamp that pinned many notes to an out-of-scale pitch.
- `setTempoRange` now preserves a song's relative tempo instead of snapping to the midpoint.
- Existing seeds now produce materially different songs from 1.0.0 while remaining deterministic for the same seed and profile.

### Fixed

- Fixed the issue where several profile rules were never being read, so the arrangement and performance logic now matches the validated configuration.
- Corrected the home-key and scale handling to keep songs anchored while allowing brief, intentional related-key movement.
- Improved layer transitions and section behaviour so arrangement changes feel purposeful and musical.

### Updated

- Upgraded project dependencies to their current versions.

---

## [1.0.0] - 2026-07-09

### Added

- Initial public release of the AlgoWASM engine and TypeScript package.
- Deterministic seeded composition, procedural synthesis, and offline rendering in WebAssembly.
- AudioWorklet-based real-time playback.
- Built-in profiles: `amiga_house_95ish`, `downtempo_breakbeat`, and `dub_deep_house`.
- Live controls for energy, intensity, mood, tempo, mute, and solo.
- MIDI export for the current generated phrase.
- Framework integration helpers for React, Vue, Svelte, and Phaser.
