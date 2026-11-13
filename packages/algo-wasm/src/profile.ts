import { AlgoWasmError } from './AlgoWasmError.js';
import { builtInProfileData } from './built-in-profiles.js';
import {
  MAX_SCALE_WEIGHT,
  MIXER_FIELDS,
  MIXER_TRACK_FIELDS,
  PATCH_FIELDS,
  SECTION_FIELDS,
  type FieldSpec
} from './profile-spec.js';
import { MAX_SCALES, WIRE_SCALE_NAMES, WIRE_TRACK_IDS, WIRE_WAVE_SHAPES } from './profile-wire.js';
import type { AlgoWasmProfile, AlgoWasmTrackId } from './types.js';

/** The built-in track ids, in the engine's fixed order. */
export const trackIds = WIRE_TRACK_IDS satisfies readonly AlgoWasmTrackId[];

/**
 * Factories for the built-in profiles. Each call returns a fresh copy, so a
 * profile can be edited without affecting later calls.
 */
export const builtInProfiles = {
  /** Mid-1990s Amiga-inspired house with a four-on-the-floor kick. */
  amigaHouse95ish(): AlgoWasmProfile {
    return cloneProfile(builtInProfileData.amigaHouse95ish);
  },
  /** Slower broken-beat profile. */
  downtempoBreakbeat(): AlgoWasmProfile {
    return cloneProfile(builtInProfileData.downtempoBreakbeat);
  },
  /** Deep, dub-flavoured house. */
  dubDeepHouse(): AlgoWasmProfile {
    return cloneProfile(builtInProfileData.dubDeepHouse);
  },
  /** Half-time, sub-led dubstep with a wobbling mid bass. */
  dubstep(): AlgoWasmProfile {
    return cloneProfile(builtInProfileData.dubstep);
  }
};

/** Returns a deep copy of a profile. */
export function cloneProfile(profile: AlgoWasmProfile): AlgoWasmProfile {
  return structuredClone(profile);
}

/**
 * Checks an untrusted value against every profile rule and returns a deep
 * copy typed as a profile.
 *
 * @throws {AlgoWasmError} With code `profile_invalid` and a message naming
 * the field and its allowed range.
 */
export function validateProfile(profile: unknown): AlgoWasmProfile {
  if (!isRecord(profile)) {
    throw invalid('Profile must be an object.');
  }

  for (const name of ['id', 'displayName', 'version'] as const) {
    if (typeof profile[name] !== 'string' || profile[name].trim() === '') {
      throw invalid(`Profile ${name} is required and must be a non-empty string.`);
    }
  }

  for (const [section, fields] of Object.entries(SECTION_FIELDS)) {
    validateFields(profile[section], fields, section);
  }

  const typed = profile as unknown as AlgoWasmProfile;
  const { tempo, limits, arrangementRules } = typed;

  if (tempo.minBpm > tempo.defaultBpm || tempo.defaultBpm > tempo.maxBpm) {
    throw invalid('tempo must be ordered: minBpm <= defaultBpm <= maxBpm.');
  }

  if (tempo.minBpm < limits.minBpm || tempo.maxBpm > limits.maxBpm) {
    throw invalid('tempo range must stay inside limits.minBpm and limits.maxBpm.');
  }

  if (limits.minSampleRate > limits.maxSampleRate) {
    throw invalid('limits.minSampleRate must not exceed limits.maxSampleRate.');
  }

  if (arrangementRules.minSectionBars > arrangementRules.maxSectionBars) {
    throw invalid('arrangementRules.minSectionBars must not exceed maxSectionBars.');
  }

  if (arrangementRules.futureBars > limits.maxBarsQueued) {
    throw invalid('arrangementRules.futureBars must not exceed limits.maxBarsQueued.');
  }

  validateScales(typed.scales);
  validateTrackList(typed.patches, PATCH_FIELDS, 'patches');

  for (const patch of typed.patches) {
    if (!WIRE_WAVE_SHAPES.includes(patch.wave)) {
      throw invalid(`patches wave must be one of ${WIRE_WAVE_SHAPES.join(', ')}.`);
    }
  }

  validateFields(typed.mixer, MIXER_FIELDS, 'mixer');
  validateTrackList(typed.mixer.tracks, MIXER_TRACK_FIELDS, 'mixer.tracks');

  return cloneProfile(typed);
}

/**
 * Converts a seed to an unsigned 64-bit integer. Larger values wrap.
 *
 * @throws {AlgoWasmError} With code `profile_invalid` when the seed is a
 * negative or fractional number, or a string that is not decimal digits.
 */
export function normaliseSeed(seed: bigint | number | string | undefined): bigint {
  if (seed === undefined) {
    return 1n;
  }

  if (typeof seed === 'bigint') {
    return BigInt.asUintN(64, seed);
  }

  if (typeof seed === 'number') {
    if (!Number.isSafeInteger(seed) || seed < 0) {
      throw invalid(
        'Seed numbers must be non-negative safe integers. Use bigint or string for larger seeds.'
      );
    }

    return BigInt(seed);
  }

  if (typeof seed !== 'string' || !/^\d+$/.test(seed)) {
    throw invalid('Seed strings must contain only decimal digits.');
  }

  return BigInt.asUintN(64, BigInt(seed));
}

/** Splits a 64-bit seed into the two 32-bit halves the engine takes. */
export function splitSeed(seed: bigint): { high: number; low: number } {
  const normalised = BigInt.asUintN(64, seed);

  return {
    high: Number((normalised >> 32n) & 0xffff_ffffn),
    low: Number(normalised & 0xffff_ffffn)
  };
}

function invalid(message: string): AlgoWasmError {
  return new AlgoWasmError('profile_invalid', message);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Checks each numeric field of one object against its table entry. */
function validateFields(value: unknown, fields: Record<string, FieldSpec>, label: string): void {
  if (!isRecord(value)) {
    throw invalid(`Profile ${label} is required and must be an object.`);
  }

  for (const [name, spec] of Object.entries(fields)) {
    const field = value[name];

    if (field === undefined && spec.optional) {
      continue;
    }

    const valid =
      typeof field === 'number' &&
      Number.isFinite(field) &&
      field >= spec.min &&
      field <= spec.max &&
      (!spec.integer || Number.isInteger(field));

    if (!valid) {
      const kind = spec.integer ? 'a whole number' : 'a number';
      const range = spec.min === spec.max ? `exactly ${spec.min}` : `from ${spec.min} to ${spec.max}`;
      throw invalid(`${label}.${name} must be ${kind} ${range}.`);
    }
  }
}

function validateScales(scales: unknown): void {
  if (!Array.isArray(scales) || scales.length === 0 || scales.length > MAX_SCALES) {
    throw invalid(`Profile scales must list between 1 and ${MAX_SCALES} scales.`);
  }

  for (const scale of scales) {
    // Unknown names are rejected: an earlier release quietly played natural
    // minor instead, which hid typing mistakes.
    if (!isRecord(scale) || !WIRE_SCALE_NAMES.includes(scale.name as never)) {
      throw invalid(`Scale name must be one of ${WIRE_SCALE_NAMES.join(', ')}.`);
    }

    const weight = scale.weight;

    if (!Number.isInteger(weight) || (weight as number) < 1 || (weight as number) > MAX_SCALE_WEIGHT) {
      throw invalid(`Scale weight must be a whole number from 1 to ${MAX_SCALE_WEIGHT}.`);
    }
  }
}

/** Checks a list that must hold exactly one valid entry per built-in track. */
function validateTrackList(list: unknown, fields: Record<string, FieldSpec>, label: string): void {
  if (!Array.isArray(list) || list.length !== trackIds.length) {
    throw invalid(`Profile ${label} must include one entry for each built-in track.`);
  }

  const seen = new Set<unknown>();

  for (const entry of list) {
    validateFields(entry, fields, label);

    if (!trackIds.includes(entry.track)) {
      throw invalid(`${label} track must be one of ${trackIds.join(', ')}.`);
    }

    seen.add(entry.track);
  }

  if (seen.size !== trackIds.length) {
    throw invalid(`Profile ${label} must not repeat a track.`);
  }
}
