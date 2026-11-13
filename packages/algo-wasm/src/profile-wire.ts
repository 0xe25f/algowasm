import type { AlgoWasmPatch, AlgoWasmProfile } from './types.js';

/**
 * Version of the flat profile layout. The engine checks it, so a package and
 * a WASM binary from different releases fail loudly instead of misreading
 * values.
 */
export const PROFILE_WIRE_VERSION = 1;

/** Largest number of weighted scales a profile may list. */
export const MAX_SCALES = 8;

/** Track ids in the order the engine stores patches and mixer entries. */
export const WIRE_TRACK_IDS = [
  'kick',
  'snare',
  'closed_hat',
  'open_hat',
  'percussion',
  'bass',
  'chords',
  'pad',
  'lead',
  'arp',
  'fx'
] as const;

/** Wave shapes in the order of their numeric ids. */
export const WIRE_WAVE_SHAPES = ['sine', 'triangle', 'saw', 'square', 'pulse', 'noise'] as const;

/** Scale names in the order of their numeric ids. */
export const WIRE_SCALE_NAMES = [
  'natural_minor',
  'dorian',
  'aeolian',
  'phrygian_dominant',
  'minor_pentatonic'
] as const;

/** Mood ids in the order of their numeric ids. */
export const WIRE_MOODS = ['dark', 'bright', 'tense', 'calm'] as const;

/** Section names in the order of their numeric ids. */
export const WIRE_SECTIONS = [
  'intro',
  'groove_a',
  'groove_b',
  'build',
  'drop',
  'variation',
  'breakdown',
  'fill',
  'transition',
  'reset'
] as const;

/**
 * Values used for the optional profile fields added in 1.2.0. Each default
 * reproduces the behaviour of earlier releases, so older profiles keep
 * composing the same songs.
 */
export const PROFILE_DEFAULTS = {
  halfTimeChance: 0.3,
  sustainChance: 0,
  detuneCents: 0,
  filterEnvOctaves: 0,
  filterEnvDecayMs: 120,
  lfoRate: 0,
  lfoDepth: 0,
  width: 0,
  duck: 0
} as const;

const PATCH_FIELDS = 18;
const HEADER_FIELDS = 41;

/** Total number of values in the flat profile layout. */
export const PROFILE_WIRE_LENGTH =
  HEADER_FIELDS + 1 + MAX_SCALES * 2 + WIRE_TRACK_IDS.length * (PATCH_FIELDS + 2);

/**
 * Encodes a validated profile as the flat `Float32Array` the engine reads.
 *
 * The layout is `[version, tempo x3, chords x4, rhythm x5, bass x5, melody x4,
 * arrangement x4, automation x3, mixer x4, limits x8, scale count,
 * (scale id, weight) x8, patches x11, mixer tracks x11]`. Patches and mixer
 * tracks are written in built-in track order whatever order the profile lists
 * them in. The engine's `Profile::from_wire` reads the same layout.
 *
 * Call `validateProfile` first: this function assumes every field is present
 * and of the right type.
 */
export function encodeProfile(profile: AlgoWasmProfile): Float32Array {
  const wire = new Float32Array(PROFILE_WIRE_LENGTH);
  let index = 0;
  const put = (...values: number[]): void => {
    for (const value of values) {
      wire[index] = value;
      index += 1;
    }
  };
  const { tempo, chordRules, rhythmRules, bassRules, melodyRules } = profile;
  const { arrangementRules, automationRules, mixer, limits } = profile;

  put(PROFILE_WIRE_VERSION);
  put(tempo.minBpm, tempo.maxBpm, tempo.defaultBpm);
  put(
    chordRules.cadenceWeight,
    chordRules.movementWeight,
    chordRules.suspendedChance,
    chordRules.brightBorrowChance
  );
  put(
    rhythmRules.kickDensity,
    rhythmRules.hatDensity,
    rhythmRules.percussionDensity,
    rhythmRules.fillChance,
    rhythmRules.halfTimeChance ?? PROFILE_DEFAULTS.halfTimeChance
  );
  put(
    bassRules.density,
    bassRules.slideChance,
    bassRules.accentChance,
    bassRules.octaveJumpChance,
    bassRules.sustainChance ?? PROFILE_DEFAULTS.sustainChance
  );
  put(
    melodyRules.density,
    melodyRules.mutationChance,
    melodyRules.callResponseChance,
    melodyRules.maxRangeSemitones
  );
  put(
    arrangementRules.minSectionBars,
    arrangementRules.maxSectionBars,
    arrangementRules.futureBars,
    arrangementRules.resetChance
  );
  put(automationRules.filterMotion, automationRules.sendMotion, automationRules.panMotion);
  put(mixer.masterGain, mixer.limiterDrive, mixer.delayFeedback, mixer.reverbMix);
  put(
    limits.minSampleRate,
    limits.maxSampleRate,
    limits.minBpm,
    limits.maxBpm,
    limits.maxTracks,
    limits.maxVoices,
    limits.maxBarsQueued,
    limits.maxEventsPerBlock
  );
  put(profile.scales.length);

  for (let slot = 0; slot < MAX_SCALES; slot += 1) {
    const scale = profile.scales[slot];
    // Unused slots are zero-filled. The engine ignores them.
    put(scale ? WIRE_SCALE_NAMES.indexOf(scale.name as (typeof WIRE_SCALE_NAMES)[number]) : 0);
    put(scale ? scale.weight : 0);
  }

  for (const track of WIRE_TRACK_IDS) {
    const patch = profile.patches.find(entry => entry.track === track) as AlgoWasmPatch;
    put(
      WIRE_WAVE_SHAPES.indexOf(patch.wave),
      patch.gain,
      patch.pan,
      patch.attackMs,
      patch.decayMs,
      patch.sustain,
      patch.releaseMs,
      patch.filterCutoff,
      patch.resonance,
      patch.delaySend,
      patch.reverbSend,
      patch.detuneCents ?? PROFILE_DEFAULTS.detuneCents,
      patch.filterEnvOctaves ?? PROFILE_DEFAULTS.filterEnvOctaves,
      patch.filterEnvDecayMs ?? PROFILE_DEFAULTS.filterEnvDecayMs,
      patch.lfoRate ?? PROFILE_DEFAULTS.lfoRate,
      patch.lfoDepth ?? PROFILE_DEFAULTS.lfoDepth,
      patch.width ?? PROFILE_DEFAULTS.width,
      patch.duck ?? PROFILE_DEFAULTS.duck
    );
  }

  for (const track of WIRE_TRACK_IDS) {
    const entry = mixer.tracks.find(candidate => candidate.track === track);
    put(entry?.gain ?? 1, entry?.pan ?? 0);
  }

  return wire;
}
