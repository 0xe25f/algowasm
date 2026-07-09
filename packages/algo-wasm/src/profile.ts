import { AlgoWasmError } from './AlgoWasmError.js';
import type { AlgoWasmProfile, AlgoWasmTrackId, AlgoWasmWaveShape } from './types.js';

export const trackIds = [
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
] as const satisfies readonly AlgoWasmTrackId[];

const waveShapes = ['sine', 'triangle', 'saw', 'square', 'pulse', 'noise'] as const;

export const builtInProfiles = {
  amigaHouse95ish(): AlgoWasmProfile {
    return cloneProfile(amigaHouse95ishProfile);
  },
  downtempoBreakbeat(): AlgoWasmProfile {
    return cloneProfile(downtempoBreakbeatProfile);
  },
  dubDeepHouse(): AlgoWasmProfile {
    return cloneProfile(dubDeepHouseProfile);
  }
};

export function cloneProfile(profile: AlgoWasmProfile): AlgoWasmProfile {
  return structuredClone(profile);
}

export function validateProfile(profile: unknown): AlgoWasmProfile {
  if (!isRecord(profile)) {
    throw new AlgoWasmError('profile_invalid', 'Profile must be an object.');
  }

  assertString(profile.id, 'Profile id is required.');
  assertString(profile.displayName, 'Profile displayName is required.');
  assertString(profile.version, 'Profile version is required.');

  const typed = profile as unknown as AlgoWasmProfile;
  validateTempo(typed);
  validateScales(typed);
  validateUnit(typed.chordRules?.suspendedChance, 'Chord suspended chance');
  validateUnit(typed.chordRules?.brightBorrowChance, 'Chord bright borrow chance');
  validateUnit(typed.rhythmRules?.kickDensity, 'Kick density');
  validateUnit(typed.rhythmRules?.hatDensity, 'Hat density');
  validateUnit(typed.rhythmRules?.percussionDensity, 'Percussion density');
  validateUnit(typed.rhythmRules?.fillChance, 'Fill chance');
  validateUnit(typed.bassRules?.density, 'Bass density');
  validateUnit(typed.bassRules?.slideChance, 'Bass slide chance');
  validateUnit(typed.bassRules?.accentChance, 'Bass accent chance');
  validateUnit(typed.bassRules?.octaveJumpChance, 'Bass octave jump chance');
  validateUnit(typed.melodyRules?.density, 'Melody density');
  validateUnit(typed.melodyRules?.mutationChance, 'Melody mutation chance');
  validateUnit(typed.melodyRules?.callResponseChance, 'Melody call and response chance');

  if (!Number.isInteger(typed.melodyRules?.maxRangeSemitones)) {
    throw new AlgoWasmError('profile_invalid', 'Melody max range must be an integer.');
  }

  validateArrangement(typed);
  validatePatches(typed);
  validateMixer(typed);
  validateLimits(typed);

  return cloneProfile(typed);
}

export function normaliseSeed(seed: bigint | number | string | undefined): bigint {
  if (seed === undefined) {
    return 1n;
  }

  if (typeof seed === 'bigint') {
    return BigInt.asUintN(64, seed);
  }

  if (typeof seed === 'number') {
    if (!Number.isSafeInteger(seed) || seed < 0) {
      throw new AlgoWasmError(
        'profile_invalid',
        'Seed numbers must be non-negative safe integers. Use bigint or string for larger seeds.'
      );
    }

    return BigInt(seed);
  }

  if (!/^\d+$/.test(seed)) {
    throw new AlgoWasmError('profile_invalid', 'Seed strings must contain only decimal digits.');
  }

  return BigInt.asUintN(64, BigInt(seed));
}

export function splitSeed(seed: bigint): { high: number; low: number } {
  const normalised = BigInt.asUintN(64, seed);

  return {
    high: Number((normalised >> 32n) & 0xffff_ffffn),
    low: Number(normalised & 0xffff_ffffn)
  };
}

function validateTempo(profile: AlgoWasmProfile): void {
  const tempo = profile.tempo;

  if (!isRecord(tempo)) {
    throw new AlgoWasmError('profile_invalid', 'Profile tempo rules are required.');
  }

  assertFiniteNumber(tempo.minBpm, 'Tempo minimum BPM');
  assertFiniteNumber(tempo.maxBpm, 'Tempo maximum BPM');
  assertFiniteNumber(tempo.defaultBpm, 'Tempo default BPM');

  if (tempo.minBpm < 40 || tempo.maxBpm > 240 || tempo.minBpm > tempo.maxBpm) {
    throw new AlgoWasmError('profile_invalid', 'Tempo range must be ordered between 40 and 240 BPM.');
  }

  if (tempo.defaultBpm < tempo.minBpm || tempo.defaultBpm > tempo.maxBpm) {
    throw new AlgoWasmError('profile_invalid', 'Default tempo must sit inside the tempo range.');
  }
}

function validateScales(profile: AlgoWasmProfile): void {
  if (!Array.isArray(profile.scales) || profile.scales.length === 0) {
    throw new AlgoWasmError('profile_invalid', 'Profile must include at least one scale.');
  }

  for (const scale of profile.scales) {
    assertString(scale.name, 'Scale name is required.');

    if (!Number.isInteger(scale.weight) || scale.weight <= 0) {
      throw new AlgoWasmError('profile_invalid', 'Scale weight must be a positive integer.');
    }
  }
}

function validateArrangement(profile: AlgoWasmProfile): void {
  const rules = profile.arrangementRules;

  if (!Number.isInteger(rules.minSectionBars) || !Number.isInteger(rules.maxSectionBars)) {
    throw new AlgoWasmError('profile_invalid', 'Arrangement section lengths must be integers.');
  }

  if (rules.minSectionBars <= 0 || rules.minSectionBars > rules.maxSectionBars) {
    throw new AlgoWasmError(
      'profile_invalid',
      'Arrangement section bars must have a valid minimum and maximum.'
    );
  }

  validateUnit(rules.resetChance, 'Arrangement reset chance');
}

function validatePatches(profile: AlgoWasmProfile): void {
  if (!Array.isArray(profile.patches) || profile.patches.length !== trackIds.length) {
    throw new AlgoWasmError(
      'profile_invalid',
      'Profile must include one patch for each built-in track.'
    );
  }

  const seen = new Set<string>();

  for (const patch of profile.patches) {
    validateTrackId(patch.track);
    validateWaveShape(patch.wave);
    validateGain(patch.gain, 'Patch gain');
    validatePan(patch.pan, 'Patch pan');
    assertFiniteNumber(patch.attackMs, 'Patch attack');
    assertFiniteNumber(patch.decayMs, 'Patch decay');
    validateUnit(patch.sustain, 'Patch sustain');
    assertFiniteNumber(patch.releaseMs, 'Patch release');
    assertFiniteNumber(patch.filterCutoff, 'Patch filter cutoff');
    validateUnit(patch.resonance, 'Patch resonance');
    validateUnit(patch.delaySend, 'Patch delay send');
    validateUnit(patch.reverbSend, 'Patch reverb send');

    if (patch.attackMs < 0 || patch.decayMs < 0 || patch.releaseMs < 0) {
      throw new AlgoWasmError('profile_invalid', 'Patch envelope values cannot be negative.');
    }

    if (patch.filterCutoff < 20 || patch.filterCutoff > 20000) {
      throw new AlgoWasmError('profile_invalid', 'Patch filter cutoff must be between 20 and 20000 Hz.');
    }

    seen.add(patch.track);
  }

  if (seen.size !== trackIds.length) {
    throw new AlgoWasmError('profile_invalid', 'Patch tracks must be unique and complete.');
  }
}

function validateMixer(profile: AlgoWasmProfile): void {
  validateGain(profile.mixer.masterGain, 'Master gain');
  validateGain(profile.mixer.limiterDrive, 'Limiter drive');
  validateUnit(profile.mixer.delayFeedback, 'Delay feedback');
  validateUnit(profile.mixer.reverbMix, 'Reverb mix');

  if (!Array.isArray(profile.mixer.tracks) || profile.mixer.tracks.length !== trackIds.length) {
    throw new AlgoWasmError(
      'profile_invalid',
      'Mixer must include one entry for each built-in track.'
    );
  }

  const seen = new Set<string>();

  for (const track of profile.mixer.tracks) {
    validateTrackId(track.track);
    validateGain(track.gain, 'Mixer track gain');
    validatePan(track.pan, 'Mixer track pan');
    seen.add(track.track);
  }

  if (seen.size !== trackIds.length) {
    throw new AlgoWasmError('profile_invalid', 'Mixer tracks must be unique and complete.');
  }
}

function validateLimits(profile: AlgoWasmProfile): void {
  const limits = profile.limits;

  for (const [name, value] of Object.entries(limits) as Array<[string, number]>) {
    if (!Number.isFinite(value)) {
      throw new AlgoWasmError('profile_invalid', `Safety limit ${name} must be a finite number.`);
    }
  }

  if (limits.minSampleRate < 22050 || limits.maxSampleRate > 96000) {
    throw new AlgoWasmError('profile_invalid', 'Sample rates must stay between 22050 and 96000 Hz.');
  }

  if (limits.maxTracks !== trackIds.length) {
    throw new AlgoWasmError('profile_invalid', 'Max tracks must match the built-in track count.');
  }
}

function validateTrackId(track: unknown): asserts track is AlgoWasmTrackId {
  if (!trackIds.includes(track as AlgoWasmTrackId)) {
    throw new AlgoWasmError('profile_invalid', 'Track id is not recognised.');
  }
}

function validateWaveShape(wave: unknown): asserts wave is AlgoWasmWaveShape {
  if (!waveShapes.includes(wave as AlgoWasmWaveShape)) {
    throw new AlgoWasmError('profile_invalid', 'Patch wave shape is not recognised.');
  }
}

function validateUnit(value: unknown, label: string): void {
  assertFiniteNumber(value, label);

  if ((value as number) < 0 || (value as number) > 1) {
    throw new AlgoWasmError('profile_invalid', `${label} must be between 0 and 1.`);
  }
}

function validateGain(value: unknown, label: string): void {
  assertFiniteNumber(value, label);

  if ((value as number) < 0 || (value as number) > 2) {
    throw new AlgoWasmError('profile_invalid', `${label} must be between 0 and 2.`);
  }
}

function validatePan(value: unknown, label: string): void {
  assertFiniteNumber(value, label);

  if ((value as number) < -1 || (value as number) > 1) {
    throw new AlgoWasmError('profile_invalid', `${label} must be between -1 and 1.`);
  }
}

function assertFiniteNumber(value: unknown, label: string): asserts value is number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new AlgoWasmError('profile_invalid', `${label} must be a finite number.`);
  }
}

function assertString(value: unknown, message: string): asserts value is string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new AlgoWasmError('profile_invalid', message);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function patch(
  track: AlgoWasmTrackId,
  wave: AlgoWasmWaveShape,
  gain: number,
  pan: number,
  attackMs: number,
  decayMs: number,
  sustain: number,
  releaseMs: number,
  filterCutoff: number,
  resonance: number,
  delaySend: number,
  reverbSend: number
) {
  return {
    track,
    wave,
    gain,
    pan,
    attackMs,
    decayMs,
    sustain,
    releaseMs,
    filterCutoff,
    resonance,
    delaySend,
    reverbSend
  };
}

const amigaHouse95ishProfile: AlgoWasmProfile = {
  id: 'amiga_house_95ish',
  displayName: 'Amiga House 95ish',
  version: '0.1.0',
  tempo: {
    minBpm: 118,
    maxBpm: 138,
    defaultBpm: 126
  },
  scales: [
    { name: 'natural_minor', weight: 34 },
    { name: 'dorian', weight: 24 },
    { name: 'aeolian', weight: 20 },
    { name: 'phrygian_dominant', weight: 10 },
    { name: 'minor_pentatonic', weight: 12 }
  ],
  chordRules: {
    cadenceWeight: 0.75,
    movementWeight: 0.6,
    suspendedChance: 0.24,
    brightBorrowChance: 0.08
  },
  rhythmRules: {
    kickDensity: 1,
    hatDensity: 0.72,
    percussionDensity: 0.28,
    fillChance: 0.32
  },
  bassRules: {
    density: 0.62,
    slideChance: 0.18,
    accentChance: 0.34,
    octaveJumpChance: 0.16
  },
  melodyRules: {
    density: 0.32,
    mutationChance: 0.22,
    callResponseChance: 0.55,
    maxRangeSemitones: 14
  },
  arrangementRules: {
    minSectionBars: 4,
    maxSectionBars: 8,
    futureBars: 8,
    resetChance: 0.08
  },
  automationRules: {
    filterMotion: 0.46,
    sendMotion: 0.24,
    panMotion: 0.18
  },
  patches: [
    patch('kick', 'sine', 1, 0, 0, 120, 0, 10, 120, 0.1, 0, 0),
    patch('snare', 'noise', 0.75, 0, 0, 90, 0, 30, 5000, 0.2, 0.05, 0.08),
    patch('closed_hat', 'noise', 0.42, 0.15, 0, 35, 0, 10, 9000, 0.2, 0.04, 0.05),
    patch('open_hat', 'noise', 0.32, 0.25, 0, 240, 0, 50, 8500, 0.2, 0.08, 0.08),
    patch('percussion', 'triangle', 0.34, -0.2, 1, 80, 0, 20, 2800, 0.3, 0.06, 0.08),
    patch('bass', 'saw', 0.72, 0, 4, 90, 0.35, 70, 680, 0.62, 0.08, 0.04),
    patch('chords', 'pulse', 0.38, -0.15, 5, 120, 0.18, 120, 1800, 0.36, 0.16, 0.18),
    patch('pad', 'triangle', 0.22, 0, 300, 700, 0.62, 600, 2400, 0.22, 0.18, 0.32),
    patch('lead', 'square', 0.36, 0.1, 8, 130, 0.24, 90, 2800, 0.28, 0.22, 0.14),
    patch('arp', 'pulse', 0.28, 0.18, 2, 80, 0.18, 50, 3400, 0.24, 0.16, 0.12),
    patch('fx', 'noise', 0.22, 0, 0, 600, 0, 120, 5500, 0.2, 0.2, 0.26)
  ],
  mixer: {
    masterGain: 0.82,
    limiterDrive: 0.96,
    delayFeedback: 0.34,
    reverbMix: 0.16,
    tracks: [
      { track: 'kick', gain: 1, pan: 0 },
      { track: 'snare', gain: 0.8, pan: 0 },
      { track: 'closed_hat', gain: 0.55, pan: 0.18 },
      { track: 'open_hat', gain: 0.45, pan: 0.28 },
      { track: 'percussion', gain: 0.42, pan: -0.24 },
      { track: 'bass', gain: 0.78, pan: 0 },
      { track: 'chords', gain: 0.5, pan: -0.12 },
      { track: 'pad', gain: 0.34, pan: 0 },
      { track: 'lead', gain: 0.48, pan: 0.08 },
      { track: 'arp', gain: 0.36, pan: 0.2 },
      { track: 'fx', gain: 0.32, pan: 0 }
    ]
  },
  limits: {
    minSampleRate: 22050,
    maxSampleRate: 96000,
    minBpm: 90,
    maxBpm: 160,
    maxTracks: 11,
    maxVoices: 48,
    maxBarsQueued: 16,
    maxEventsPerBlock: 128
  }
};

const downtempoBreakbeatProfile: AlgoWasmProfile = {
  id: 'downtempo_breakbeat',
  displayName: 'Downtempo Breakbeat',
  version: '0.1.0',
  tempo: {
    minBpm: 88,
    maxBpm: 112,
    defaultBpm: 98
  },
  scales: [
    { name: 'natural_minor', weight: 26 },
    { name: 'dorian', weight: 30 },
    { name: 'aeolian', weight: 16 },
    { name: 'phrygian_dominant', weight: 16 },
    { name: 'minor_pentatonic', weight: 12 }
  ],
  chordRules: {
    cadenceWeight: 0.7,
    movementWeight: 0.5,
    suspendedChance: 0.3,
    brightBorrowChance: 0.05
  },
  rhythmRules: {
    kickDensity: 0.8,
    hatDensity: 0.6,
    percussionDensity: 0.5,
    fillChance: 0.55
  },
  bassRules: {
    density: 0.55,
    slideChance: 0.14,
    accentChance: 0.4,
    octaveJumpChance: 0.12
  },
  melodyRules: {
    density: 0.26,
    mutationChance: 0.22,
    callResponseChance: 0.5,
    maxRangeSemitones: 12
  },
  arrangementRules: {
    minSectionBars: 4,
    maxSectionBars: 8,
    futureBars: 8,
    resetChance: 0.07
  },
  automationRules: {
    filterMotion: 0.5,
    sendMotion: 0.3,
    panMotion: 0.16
  },
  patches: [
    patch('kick', 'sine', 0.95, 0, 0, 130, 0, 130, 100, 0.1, 0, 0.05),
    patch('snare', 'noise', 0.7, 0, 0, 100, 0, 40, 4200, 0.2, 0.1, 0.18),
    patch('closed_hat', 'noise', 0.36, 0.12, 0, 30, 0, 8, 8000, 0.2, 0.05, 0.1),
    patch('open_hat', 'noise', 0.3, 0.22, 0, 220, 0, 45, 7500, 0.2, 0.1, 0.14),
    patch('percussion', 'triangle', 0.4, -0.22, 1, 90, 0, 25, 2400, 0.3, 0.12, 0.16),
    patch('bass', 'sine', 0.75, 0, 6, 110, 0.45, 90, 520, 0.3, 0.06, 0.08),
    patch('chords', 'triangle', 0.34, -0.14, 20, 160, 0.3, 220, 1500, 0.28, 0.22, 0.3),
    patch('pad', 'triangle', 0.26, 0, 400, 800, 0.65, 900, 1800, 0.2, 0.24, 0.42),
    patch('lead', 'triangle', 0.3, 0.1, 14, 140, 0.22, 160, 2200, 0.24, 0.28, 0.22),
    patch('arp', 'pulse', 0.22, 0.18, 3, 70, 0.16, 55, 3000, 0.22, 0.2, 0.16),
    patch('fx', 'noise', 0.2, 0, 0, 700, 0, 150, 5000, 0.2, 0.26, 0.34)
  ],
  mixer: {
    masterGain: 0.8,
    limiterDrive: 0.9,
    delayFeedback: 0.4,
    reverbMix: 0.3,
    tracks: [
      { track: 'kick', gain: 1, pan: 0 },
      { track: 'snare', gain: 0.78, pan: 0 },
      { track: 'closed_hat', gain: 0.5, pan: 0.18 },
      { track: 'open_hat', gain: 0.42, pan: 0.28 },
      { track: 'percussion', gain: 0.46, pan: -0.24 },
      { track: 'bass', gain: 0.76, pan: 0 },
      { track: 'chords', gain: 0.46, pan: -0.12 },
      { track: 'pad', gain: 0.4, pan: 0 },
      { track: 'lead', gain: 0.4, pan: 0.08 },
      { track: 'arp', gain: 0.32, pan: 0.2 },
      { track: 'fx', gain: 0.3, pan: 0 }
    ]
  },
  limits: {
    minSampleRate: 22050,
    maxSampleRate: 96000,
    minBpm: 70,
    maxBpm: 130,
    maxTracks: 11,
    maxVoices: 48,
    maxBarsQueued: 16,
    maxEventsPerBlock: 128
  }
};

const dubDeepHouseProfile: AlgoWasmProfile = {
  id: 'dub_deep_house',
  displayName: 'Dub Deep House',
  version: '0.1.0',
  tempo: {
    minBpm: 118,
    maxBpm: 124,
    defaultBpm: 121
  },
  scales: [
    { name: 'natural_minor', weight: 30 },
    { name: 'dorian', weight: 30 },
    { name: 'aeolian', weight: 24 },
    { name: 'phrygian_dominant', weight: 6 },
    { name: 'minor_pentatonic', weight: 10 }
  ],
  chordRules: {
    cadenceWeight: 0.8,
    movementWeight: 0.4,
    suspendedChance: 0.3,
    brightBorrowChance: 0.1
  },
  rhythmRules: {
    kickDensity: 1,
    hatDensity: 0.5,
    percussionDensity: 0.22,
    fillChance: 0.18
  },
  bassRules: {
    density: 0.48,
    slideChance: 0.05,
    accentChance: 0.22,
    octaveJumpChance: 0.05
  },
  melodyRules: {
    density: 0.2,
    mutationChance: 0.18,
    callResponseChance: 0.4,
    maxRangeSemitones: 10
  },
  arrangementRules: {
    minSectionBars: 8,
    maxSectionBars: 16,
    futureBars: 12,
    resetChance: 0.05
  },
  automationRules: {
    filterMotion: 0.6,
    sendMotion: 0.42,
    panMotion: 0.24
  },
  patches: [
    patch('kick', 'sine', 1, 0, 0, 130, 0, 140, 110, 0.1, 0, 0.06),
    patch('snare', 'noise', 0.6, 0, 0, 80, 0, 30, 4500, 0.2, 0.08, 0.22),
    patch('closed_hat', 'noise', 0.4, 0.16, 0, 28, 0, 8, 8200, 0.2, 0.05, 0.12),
    patch('open_hat', 'noise', 0.3, 0.26, 0, 200, 0, 40, 7800, 0.2, 0.1, 0.16),
    patch('percussion', 'triangle', 0.3, -0.2, 1, 70, 0, 20, 2600, 0.28, 0.1, 0.14),
    patch('bass', 'triangle', 0.8, 0, 8, 120, 0.55, 110, 420, 0.2, 0.04, 0.06),
    patch('chords', 'triangle', 0.3, -0.1, 40, 200, 0.4, 380, 1200, 0.24, 0.3, 0.38),
    patch('pad', 'triangle', 0.28, 0, 600, 900, 0.7, 1100, 1600, 0.18, 0.26, 0.46),
    patch('lead', 'triangle', 0.24, 0.06, 20, 150, 0.2, 200, 1900, 0.2, 0.32, 0.3),
    patch('arp', 'triangle', 0.2, 0.16, 4, 80, 0.14, 70, 2600, 0.2, 0.26, 0.22),
    patch('fx', 'noise', 0.18, 0, 0, 800, 0, 180, 4800, 0.2, 0.3, 0.4)
  ],
  mixer: {
    masterGain: 0.78,
    limiterDrive: 0.85,
    delayFeedback: 0.46,
    reverbMix: 0.36,
    tracks: [
      { track: 'kick', gain: 1, pan: 0 },
      { track: 'snare', gain: 0.62, pan: 0 },
      { track: 'closed_hat', gain: 0.46, pan: 0.18 },
      { track: 'open_hat', gain: 0.38, pan: 0.28 },
      { track: 'percussion', gain: 0.34, pan: -0.24 },
      { track: 'bass', gain: 0.8, pan: 0 },
      { track: 'chords', gain: 0.4, pan: -0.12 },
      { track: 'pad', gain: 0.36, pan: 0 },
      { track: 'lead', gain: 0.34, pan: 0.08 },
      { track: 'arp', gain: 0.28, pan: 0.2 },
      { track: 'fx', gain: 0.26, pan: 0 }
    ]
  },
  limits: {
    minSampleRate: 22050,
    maxSampleRate: 96000,
    minBpm: 100,
    maxBpm: 130,
    maxTracks: 11,
    maxVoices: 48,
    maxBarsQueued: 24,
    maxEventsPerBlock: 128
  }
};
