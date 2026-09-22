# Algorithms

AlgoWASM is a spiritual successor to early algorithmic electronic music tools. It does not copy their code, samples, or data.

The rules described below are shared by every built-in profile. See [profiles.md](profiles.md) for how `amiga_house_95ish`, `downtempo_breakbeat`, and `dub_deep_house` each configure these rules and their patches differently.

## Seeded randomness

The core uses xoshiro256** seeded by splitmix64. The same seed and profile produce the same generated event stream for the same engine version.

## Song identity

Each seed first draws a song identity, called the genome (`crates/algo-wasm-core/src/genome.rs`). It is drawn once, from its own random stream, and holds everything that should stay recognisable for the whole song:

- Tempo, placed inside the profile's `minBpm` to `maxBpm` range and leaning towards `defaultBpm`.
- A home key and scale.
- A groove: kick, snare, ghost note, hat, open hat, stab, and percussion placements, plus swing.
- A lead motif and a bassline shape.
- A signature chord progression and an alternative progression.
- Arpeggio shape and rate, stab length, and which layers each section brings in.
- A drum kit: kick pitch, sweep, and decay, snare tone, hat partials, and so on.
- Moderate per-song changes to the profile's tonal patches: cutoff, resonance, envelope, sends, and sometimes a wave swap within the same family (saw, square, and pulse swap with each other; sine and triangle swap with each other).
- Slow automation shapes for each track.

Sections then vary this material instead of inventing unrelated material, so a song stays coherent and two seeds stay clearly apart.

## Arrangement

The engine generates sections such as intro, groove, build, drop, variation, breakdown, fill, transition, and reset. The next section is a seeded, weighted choice that energy tilts towards builds and drops or towards breakdowns. Section lengths prefer whole four-bar phrases.

Sections bring layers in and out. For example, a breakdown drops the kick and stabs, and a build holds the lead back while its snare rolls up. Each section plays a four-bar A phrase, then a B variation of it, and may end on a fill.

## Harmony

Songs stay in their home key. During high points they occasionally move to a related key for one section, then return home.

Chords are stacked in thirds from the key's scale, so Dorian, Phrygian dominant, and natural minor songs have audibly different chords. The minor pentatonic scale borrows its natural minor parent for chords. Each chord is voiced by picking the inversion that moves least from the previous chord. `chordRules` weight the choice of progression and control suspended and borrowed major chords.

## Rhythm

The base grid is 16 steps per bar. Profiles with a `kickDensity` of 1 keep a four-on-the-floor kick. Lower densities pick a broken kick pattern instead. Snares land on beats 2 and 4, or on beat 3 for a half-time feel, with seeded ghost notes. Hats and percussion use seeded Euclidean distributions. Swing delays every second 16th step.

## Bass

Basslines are monophonic. They repeat a seeded one-bar shape of roots, fifths, octaves, and passing tones, moved onto each chord. Slides, accents, rests, and octave jumps create acid-inspired motion without copying samples.

## Melody

The lead repeats the song motif as call, response, call, and cadence. `callResponseChance` decides whether the response answers the call, and `mutationChance` decides how far each section strays from the motif. Notes always stay in the key's scale.

## Automation

Each tonal track has a slow seeded LFO that moves its filter, sends, and pan, scaled by `automationRules`. Sections shape the filter as well: builds open it, breakdowns close it, and drops push it open a little. Energy and mood tilt the overall brightness.

## Synthesis

The engine uses procedural oscillators, envelopes, a resonant state-variable filter, filtered noise, simple saturation, delay, DC blocking, and a soft limiter. Drums are synthesised from sine, triangle, noise, and metallic oscillator mixtures.
