# Algorithms

AlgoWASM is a spiritual successor to early algorithmic electronic music tools. It does not copy their code, samples, or data.

The rules described below are shared by every built-in profile. See [profiles.md](profiles.md) for how `amiga_house_95ish`, `downtempo_breakbeat`, and `dub_deep_house` each configure these rules and their patches differently.

## Seeded randomness

The core uses xoshiro256** seeded by splitmix64. The same seed and profile produce the same generated event stream for the same engine version.

## Arrangement

The engine generates sections such as intro, groove, build, drop, variation, breakdown, fill, transition, and reset. Sections have density and energy curves. The generator keeps musical memory by recalling and mutating previous phrases.

## Harmony

Each profile chooses roots and scales from weighted rules. It uses reliable minor and modal chord movements such as `i - VI - VII - i` and `i - iv - VII - VI`.

## Rhythm

The base grid is 16 steps per bar. The kick favours four-on-the-floor. Snare or clap hits land on beats 2 and 4. Hats and percussion use density rules and Euclidean distributions.

## Bass

Basslines are monophonic. They prefer roots, fifths, octaves, and chord tones. Slides, accents, rests, and octave jumps create acid-inspired motion without copying samples.

## Melody

Lead motifs are short and restrained. They use chord tones on strong positions, scale tones on weaker positions, and phrase recall for call and response.

## Synthesis

The engine uses procedural oscillators, envelopes, filtered noise, simple saturation, delay, DC blocking, and a soft limiter. Drums are synthesised from sine, triangle, noise, and metallic oscillator mixtures.
