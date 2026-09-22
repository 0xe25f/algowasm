# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Songs from different seeds now sound clearly different. Each seed draws a song identity: its own tempo inside the profile range, a home key, a groove with swing, a lead motif, a bassline shape, chord progressions, a drum kit, patch variations, and automation shapes.
- **Breaking:** an existing seed produces a different song than in 1.0.0. The same seed and profile still always produce the same song.
- Chords are now built from the chosen scale and voiced with smooth voice leading, instead of always being minor seventh chords.
- The lead follows a motif as call, response, call, and cadence, and always stays in key. Previously most lead notes were clamped onto one pitch.
- Songs keep a home key, with occasional short moves to a related key, instead of changing key every section.
- Sections bring layers in and out, play an A phrase then a B variation, and end on one of several fill styles.
- The arrangement branches with seeded, energy-weighted choices instead of following one fixed path.
- `setTempoRange` keeps the song's relative tempo inside the new range instead of always picking the midpoint.

### Fixed

- `chordRules`, `automationRules`, `melodyRules.mutationChance`, and `melodyRules.callResponseChance` now affect the music. They were previously validated but ignored.
- Patch `resonance` now has an audible effect through a new resonant state-variable filter.
- `rhythmRules.fillChance` now applies to the end of each section rather than to every fourth bar.

## [1.0.0] - 2026-07-09

### Added

- Initial public release of the AlgoWASM engine and TypeScript package.
- Deterministic seeded composition, procedural synthesis, and offline rendering in WebAssembly.
- AudioWorklet-based real-time playback.
- Built-in profiles: `amiga_house_95ish`, `downtempo_breakbeat`, and `dub_deep_house`.
- Live controls for energy, intensity, mood, tempo, mute, and solo.
- MIDI export for the current generated phrase.
- Framework integration helpers for React, Vue, Svelte, and Phaser.
