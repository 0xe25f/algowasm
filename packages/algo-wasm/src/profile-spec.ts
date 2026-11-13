import { MAX_SCALES, WIRE_SCALE_NAMES, WIRE_TRACK_IDS, WIRE_WAVE_SHAPES } from './profile-wire.js';

/** The allowed range of one numeric profile field. */
export interface FieldSpec {
  min: number;
  max: number;
  /** True when the value must be a whole number. */
  integer?: boolean;
  /** True when the field may be left out. */
  optional?: boolean;
}

const unit: FieldSpec = { min: 0, max: 1 };
const optionalUnit: FieldSpec = { min: 0, max: 1, optional: true };
const gain: FieldSpec = { min: 0, max: 2 };
const pan: FieldSpec = { min: -1, max: 1 };
const bpm: FieldSpec = { min: 40, max: 240 };
const bars: FieldSpec = { min: 1, max: 64, integer: true };
const milliseconds: FieldSpec = { min: 0, max: 10000 };

/**
 * Every numeric field in a profile section, with its allowed range.
 *
 * This table is the single description of the profile's numeric limits. The
 * runtime validator reads it, and the published JSON schema is generated from
 * it, so the two cannot disagree. The engine repeats the same limits as a
 * second line of defence.
 */
export const SECTION_FIELDS = {
  tempo: { minBpm: bpm, maxBpm: bpm, defaultBpm: bpm },
  chordRules: {
    cadenceWeight: { min: 0, max: 4 },
    movementWeight: { min: 0, max: 4 },
    suspendedChance: unit,
    brightBorrowChance: unit
  },
  rhythmRules: {
    kickDensity: unit,
    hatDensity: unit,
    percussionDensity: unit,
    fillChance: unit,
    halfTimeChance: optionalUnit
  },
  bassRules: {
    density: unit,
    slideChance: unit,
    accentChance: unit,
    octaveJumpChance: unit,
    sustainChance: optionalUnit
  },
  melodyRules: {
    density: unit,
    mutationChance: unit,
    callResponseChance: unit,
    maxRangeSemitones: { min: 1, max: 48, integer: true }
  },
  arrangementRules: {
    minSectionBars: bars,
    maxSectionBars: bars,
    futureBars: bars,
    resetChance: unit
  },
  automationRules: { filterMotion: unit, sendMotion: unit, panMotion: unit },
  limits: {
    minSampleRate: { min: 22050, max: 96000, integer: true },
    maxSampleRate: { min: 22050, max: 96000, integer: true },
    minBpm: { min: 1, max: 1000 },
    maxBpm: { min: 1, max: 1000 },
    maxTracks: { min: WIRE_TRACK_IDS.length, max: WIRE_TRACK_IDS.length, integer: true },
    maxVoices: { min: 1, max: 64, integer: true },
    maxBarsQueued: { min: 1, max: 64, integer: true },
    maxEventsPerBlock: { min: 1, max: 512, integer: true }
  }
} as const satisfies Record<string, Record<string, FieldSpec>>;

/** Numeric fields of the mixer, apart from its per-track entries. */
export const MIXER_FIELDS = {
  masterGain: gain,
  limiterDrive: gain,
  delayFeedback: unit,
  reverbMix: unit
} as const satisfies Record<string, FieldSpec>;

/** Numeric fields of one patch. */
export const PATCH_FIELDS = {
  gain,
  pan,
  attackMs: milliseconds,
  decayMs: milliseconds,
  sustain: unit,
  releaseMs: milliseconds,
  filterCutoff: { min: 20, max: 20000 },
  resonance: unit,
  delaySend: unit,
  reverbSend: unit,
  detuneCents: { min: 0, max: 50, optional: true },
  filterEnvOctaves: { min: 0, max: 6, optional: true },
  filterEnvDecayMs: { min: 1, max: 10000, optional: true },
  lfoRate: { min: 0, max: 8, optional: true },
  lfoDepth: { min: 0, max: 4, optional: true },
  width: optionalUnit,
  duck: optionalUnit
} as const satisfies Record<string, FieldSpec>;

/** Numeric fields of one mixer track entry. */
export const MIXER_TRACK_FIELDS = { gain, pan } as const satisfies Record<string, FieldSpec>;

/** Largest allowed scale weight. */
export const MAX_SCALE_WEIGHT = 1000000;

type JsonSchema = Record<string, unknown>;

function numberSchema(spec: FieldSpec): JsonSchema {
  return { type: spec.integer ? 'integer' : 'number', minimum: spec.min, maximum: spec.max };
}

function objectSchema(
  fields: Record<string, FieldSpec>,
  extra: Record<string, JsonSchema> = {}
): JsonSchema {
  const properties: Record<string, JsonSchema> = { ...extra };
  const required = Object.keys(extra);

  for (const [name, spec] of Object.entries(fields)) {
    properties[name] = numberSchema(spec);

    if (!spec.optional) {
      required.push(name);
    }
  }

  return { type: 'object', required, properties, additionalProperties: false };
}

/**
 * Builds the JSON schema for a profile from the field tables.
 *
 * The build writes the result to `dist/profile-schema.json`. A schema cannot
 * express every rule, such as the tempo range being ordered, so
 * `validateProfile` remains the authority.
 */
export function buildProfileSchema(): JsonSchema {
  const text: JsonSchema = { type: 'string', minLength: 1 };
  const track: JsonSchema = { enum: [...WIRE_TRACK_IDS] };
  const trackCount = WIRE_TRACK_IDS.length;
  const properties: Record<string, JsonSchema> = { id: text, displayName: text, version: text };

  for (const [name, fields] of Object.entries(SECTION_FIELDS)) {
    properties[name] = objectSchema(fields);
  }

  properties.scales = {
    type: 'array',
    minItems: 1,
    maxItems: MAX_SCALES,
    items: {
      type: 'object',
      required: ['name', 'weight'],
      properties: {
        name: { enum: [...WIRE_SCALE_NAMES] },
        weight: { type: 'integer', minimum: 1, maximum: MAX_SCALE_WEIGHT }
      },
      additionalProperties: false
    }
  };
  properties.patches = {
    type: 'array',
    minItems: trackCount,
    maxItems: trackCount,
    items: objectSchema(PATCH_FIELDS, { track, wave: { enum: [...WIRE_WAVE_SHAPES] } })
  };
  properties.mixer = objectSchema(MIXER_FIELDS, {
    tracks: {
      type: 'array',
      minItems: trackCount,
      maxItems: trackCount,
      items: objectSchema(MIXER_TRACK_FIELDS, { track })
    }
  });

  return {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    $id: 'https://algo-wasm.invalid/schema/profile.schema.json',
    title: 'AlgoWASM profile',
    type: 'object',
    additionalProperties: false,
    required: Object.keys(properties),
    properties
  };
}
