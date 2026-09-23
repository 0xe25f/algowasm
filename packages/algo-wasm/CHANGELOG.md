# Changelog

All notable changes to this package are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


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

- Initial public release of `@algo-wasm/algo-wasm`.
- Deterministic seeded composition, procedural synthesis, and offline rendering in WebAssembly.
- AudioWorklet-based real-time playback.
- Built-in profiles: `amiga_house_95ish`, `downtempo_breakbeat`, and `dub_deep_house`.
- Live controls for energy, intensity, mood, tempo, mute, and solo.
- MIDI export for the current generated phrase.
- Framework integration helpers for React, Vue, Svelte, and Phaser.
