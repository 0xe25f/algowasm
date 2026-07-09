# Profiles

## What Is A Profile

A profile is a JSON-serialisable style definition. It controls tempo, scale
choices, chord grammar, rhythm density, bassline behaviour, melody behaviour,
arrangement pacing, automation movement, per-track synth patches, mixer
settings, and safety limits. The same seed and profile always produce the
same generated event stream.

Profiles never contain audio samples. Every sound comes from the procedural
synth engine in `crates/algo-wasm-core`.

## Built-In Profiles

AlgoWASM ships three built-in profiles through `builtInProfiles`:

| Profile | `id` | Tempo | Character |
| --- | --- | --- | --- |
| `builtInProfiles.amigaHouse95ish()` | `amiga_house_95ish` | 118 to 138 BPM | The default mid-1990s Amiga-inspired house profile. Four-on-the-floor kick, syncopated hats, acid-leaning saw bass, bright pulse stabs. |
| `builtInProfiles.downtempoBreakbeat()` | `downtempo_breakbeat` | 88 to 112 BPM | A slower, broken-beat profile. Lower kick density, more percussion, and a high fill chance for frequent snare rolls. Same chord grammar as the default profile. |
| `builtInProfiles.dubDeepHouse()` | `dub_deep_house` | 118 to 124 BPM | A roomier, more hypnotic profile. Longer sections, sparser percussion, a round sine or triangle bass instead of a bright saw, and heavier delay and reverb sends. |

`examples/vanilla`, `examples/react`, and `examples/phaser` all include a Profile dropdown that populates its option labels from each profile's own `displayName` and calls `setProfile()` live while a track plays.

```ts
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

const player = await AlgoWasmPlayer.create({
  wasmUrl: '/algo_wasm.wasm',
  workletUrl: '/algo-worklet.js',
  profile: builtInProfiles.dubDeepHouse(),
  seed: 42n
});

await player.start();
```

## Profile Fields

- `id`, `displayName`, `version`: stable identity fields. Keep `id` unique and unchanged once released.
- `tempo`: `minBpm`, `maxBpm`, and `defaultBpm`. Must sit inside the profile's own `limits.minBpm` and `limits.maxBpm`.
- `scales`: a weighted list of scale names. At least one scale is required. Higher weight means the scale is picked more often.
- `chordRules`: `cadenceWeight` and `movementWeight` shape progression scoring. `suspendedChance` and `brightBorrowChance` control how often suspended and borrowed major chords appear.
- `rhythmRules`: `kickDensity`, `hatDensity`, and `percussionDensity` control how often each drum fires on its fixed grid position. `fillChance` controls how often the final bar of a 4-bar pattern rolls into a fill.
- `bassRules`: `density`, `slideChance`, `accentChance`, and `octaveJumpChance` shape the monophonic bassline.
- `melodyRules`: `density`, `mutationChance`, `callResponseChance`, and `maxRangeSemitones` shape lead motifs.
- `arrangementRules`: `minSectionBars` and `maxSectionBars` bound section length. `futureBars` controls how many bars stay queued ahead of playback. `resetChance` controls how often the arrangement returns to a fresh section instead of continuing to build.
- `automationRules`: `filterMotion`, `sendMotion`, and `panMotion` scale slow automation movement.
- `patches`: one entry per built-in track (`kick`, `snare`, `closed_hat`, `open_hat`, `percussion`, `bass`, `chords`, `pad`, `lead`, `arp`, `fx`). Each patch sets `wave`, `gain`, `pan`, envelope timings, `filterCutoff`, `resonance`, `delaySend`, and `reverbSend`.
- `mixer`: `masterGain`, `limiterDrive`, `delayFeedback`, `reverbMix`, and one gain and pan entry per track.
- `limits`: safety bounds for sample rate, tempo, track count, voice count, queued bars, and events per block. Profile fields are validated against these limits, not against fixed global constants.

## What Actually Changes The Sound

The generative rule groups (`chordRules`, `rhythmRules`, `bassRules`, `melodyRules`, `arrangementRules`) decide *what* the engine plays: which notes, which rhythm steps, and which sections come next. Given the same seed, two profiles with identical rule groups generate an identical event stream.

The `patches` and `mixer` fields decide *how it sounds*: oscillator shape, envelope, filter cutoff, panning, and send levels. `dub_deep_house` demonstrates this directly. Its rule groups stay close to `amiga_house_95ish`, but its patches swap the bright saw bass and pulse stabs for rounder triangle and sine sources with longer envelopes and heavier reverb and delay sends.

### Known limitation: `resonance` is not yet applied

Each patch has a `resonance` field, and it is validated to stay between 0 and 1. The current DSP filter (`OnePole` in `crates/algo-wasm-core/src/dsp.rs`) is a one-pole low-pass filter with a cutoff but no resonance or Q control, so `resonance` has no audible effect yet. Filter movement in the built-in profiles comes from `filterCutoff`, `automationRules.filterMotion`, and the accent boost applied to bass notes. A genuinely resonant, self-oscillating filter (the sound behind classic acid basslines) would need a new filter implementation before `resonance` can do anything.

## Choosing A Profile By Genre

- Want the original house sound: `builtInProfiles.amigaHouse95ish()`.
- Want a slower, broken-beat groove: `builtInProfiles.downtempoBreakbeat()`.
- Want a roomy, hypnotic deep house or dub sound: `builtInProfiles.dubDeepHouse()`.

## Building A Custom Profile

Clone a built-in profile and adjust fields, or author a full object that matches [`profile-schema.json`](../packages/algo-wasm/src/profile-schema.json):

```ts
import { builtInProfiles, validateProfile } from '@algo-wasm/algo-wasm';

const profile = builtInProfiles.dubDeepHouse();
profile.id = 'dub_deep_house_slow';
profile.tempo.minBpm = 110;
profile.tempo.maxBpm = 118;
profile.tempo.defaultBpm = 114;

const validated = validateProfile(profile);
```

`validateProfile()` throws an `AlgoWasmError` with code `profile_invalid` and a specific British English message if any field is missing, out of range, or inconsistent. Validation runs before the profile ever reaches WASM.

When adding a new built-in profile to the library itself, mirror the same profile in both places so Rust and TypeScript consumers stay in sync:

- `crates/algo-wasm-core/src/profile.rs`: add a `pub fn` returning a `Profile`, plus dedicated patch and mixer helper functions.
- `packages/algo-wasm/src/profile.ts`: add a matching `AlgoWasmProfile` constant and register it on `builtInProfiles`.

Add a Rust `#[test]` that calls `.validate()` on the new profile, and a package test that renders a short buffer through the compiled WASM and asserts the output is finite and non-silent. This catches unsafe envelope, filter, or gain values before release.

## Validation And Safety

Profile validation enforces:

- non-empty `id`, `displayName`, and `version`;
- at least one scale with a positive integer weight;
- an ordered tempo range that sits inside the profile's own safety limits and within 40 to 240 BPM overall;
- every unit-range field (chances, densities, sustain, sends) between 0 and 1;
- every gain field between 0 and 2, every pan field between -1 and 1;
- filter cutoff between 20 and 20,000 Hz;
- exactly one patch and one mixer entry per built-in track;
- sample rate limits between 22,050 and 96,000 Hz;
- arrangement `futureBars` no larger than `limits.maxBarsQueued`.

A profile that fails any of these checks is rejected with a clear error before it reaches the render path, so invalid profiles cannot produce NaN, infinite, or runaway audio output.
