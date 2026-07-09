export type AlgoWasmErrorCode =
  | 'browser_feature_missing'
  | 'audio_context_locked'
  | 'wasm_load_failed'
  | 'worklet_load_failed'
  | 'profile_invalid'
  | 'engine_failed'
  | 'render_failed'
  | 'destroyed';

export type AlgoWasmEventName =
  | 'ready'
  | 'started'
  | 'paused'
  | 'resumed'
  | 'stopped'
  | 'section-change'
  | 'bar'
  | 'beat'
  | 'error'
  | 'warning'
  | 'metrics';

export type AlgoWasmTrackId =
  | 'kick'
  | 'snare'
  | 'closed_hat'
  | 'open_hat'
  | 'percussion'
  | 'bass'
  | 'chords'
  | 'pad'
  | 'lead'
  | 'arp'
  | 'fx';

export type AlgoWasmMood = 'dark' | 'bright' | 'tense' | 'calm';

export type AlgoWasmWaveShape = 'sine' | 'triangle' | 'saw' | 'square' | 'pulse' | 'noise';

export interface AlgoWasmTempoRules {
  minBpm: number;
  maxBpm: number;
  defaultBpm: number;
}

export interface AlgoWasmScaleRule {
  name: string;
  weight: number;
}

export interface AlgoWasmChordRules {
  cadenceWeight: number;
  movementWeight: number;
  suspendedChance: number;
  brightBorrowChance: number;
}

export interface AlgoWasmRhythmRules {
  kickDensity: number;
  hatDensity: number;
  percussionDensity: number;
  fillChance: number;
}

export interface AlgoWasmBassRules {
  density: number;
  slideChance: number;
  accentChance: number;
  octaveJumpChance: number;
}

export interface AlgoWasmMelodyRules {
  density: number;
  mutationChance: number;
  callResponseChance: number;
  maxRangeSemitones: number;
}

export interface AlgoWasmArrangementRules {
  minSectionBars: number;
  maxSectionBars: number;
  futureBars: number;
  resetChance: number;
}

export interface AlgoWasmAutomationRules {
  filterMotion: number;
  sendMotion: number;
  panMotion: number;
}

export interface AlgoWasmPatch {
  track: AlgoWasmTrackId;
  wave: AlgoWasmWaveShape;
  gain: number;
  pan: number;
  attackMs: number;
  decayMs: number;
  sustain: number;
  releaseMs: number;
  filterCutoff: number;
  resonance: number;
  delaySend: number;
  reverbSend: number;
}

export interface AlgoWasmMixerTrackProfile {
  track: AlgoWasmTrackId;
  gain: number;
  pan: number;
}

export interface AlgoWasmMixerProfile {
  masterGain: number;
  limiterDrive: number;
  delayFeedback: number;
  reverbMix: number;
  tracks: AlgoWasmMixerTrackProfile[];
}

export interface AlgoWasmSafetyLimits {
  minSampleRate: number;
  maxSampleRate: number;
  minBpm: number;
  maxBpm: number;
  maxTracks: number;
  maxVoices: number;
  maxBarsQueued: number;
  maxEventsPerBlock: number;
}

export interface AlgoWasmProfile {
  id: string;
  displayName: string;
  version: string;
  tempo: AlgoWasmTempoRules;
  scales: AlgoWasmScaleRule[];
  chordRules: AlgoWasmChordRules;
  rhythmRules: AlgoWasmRhythmRules;
  bassRules: AlgoWasmBassRules;
  melodyRules: AlgoWasmMelodyRules;
  arrangementRules: AlgoWasmArrangementRules;
  automationRules: AlgoWasmAutomationRules;
  patches: AlgoWasmPatch[];
  mixer: AlgoWasmMixerProfile;
  limits: AlgoWasmSafetyLimits;
}

export interface AlgoWasmTrackSnapshot {
  id: AlgoWasmTrackId;
  muted: boolean;
  solo: boolean;
  level: number;
}

export interface AlgoWasmSnapshot {
  seed: string;
  sampleRate: number;
  bpm: number;
  currentBar: number;
  currentBeat: number;
  currentSection: string;
  energy: number;
  intensity: number;
  isPlaying: boolean;
  tracks: AlgoWasmTrackSnapshot[];
}

export interface AlgoWasmMetrics {
  renderedFrames: number;
  activeVoices: number;
  droppedVoices: number;
  recentEvents: number;
}

export type AlgoWasmGeneratedEvent =
  | {
      type: 'note-on';
      sample: number;
      track: AlgoWasmTrackId;
      note: number;
      velocity: number;
    }
  | {
      type: 'note-off';
      sample: number;
      track: AlgoWasmTrackId;
      note: number;
    }
  | {
      type: 'control';
      sample: number;
      track: AlgoWasmTrackId;
      name: string;
      value: number;
    }
  | {
      type: 'section-change';
      sample: number;
      section: string;
      bar: number;
    }
  | {
      type: 'bar';
      sample: number;
      bar: number;
    }
  | {
      type: 'beat';
      sample: number;
      bar: number;
      beat: number;
    };

export type AlgoWasmEventPayloads = {
  ready: AlgoWasmSnapshot;
  started: AlgoWasmSnapshot;
  paused: AlgoWasmSnapshot;
  resumed: AlgoWasmSnapshot;
  stopped: AlgoWasmSnapshot;
  'section-change': AlgoWasmGeneratedEvent;
  bar: AlgoWasmGeneratedEvent;
  beat: AlgoWasmGeneratedEvent;
  error: Error;
  warning: string;
  metrics: AlgoWasmMetrics;
};

export type AlgoWasmEventHandler<Name extends AlgoWasmEventName> = (
  payload: AlgoWasmEventPayloads[Name]
) => void;

export interface AlgoWasmPlayerOptions {
  wasmUrl: string | URL;
  workletUrl: string | URL;
  profile?: AlgoWasmProfile;
  seed?: bigint | number | string;
  volume?: number;
  audioContext?: AudioContext;
}

export interface AlgoWasmOfflineRenderOptions {
  seconds: number;
  sampleRate?: number;
  seed?: bigint | number | string;
  profile?: AlgoWasmProfile;
}

export interface AlgoWasmMidiExportOptions {
  downloadName?: string;
}
