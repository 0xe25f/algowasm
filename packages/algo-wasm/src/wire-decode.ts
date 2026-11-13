import { WIRE_SECTIONS, WIRE_TRACK_IDS } from './profile-wire.js';
import type { AlgoWasmGeneratedEvent, AlgoWasmSnapshot } from './types.js';

/** Number of values each event occupies in the engine's event buffer. */
export const EVENT_WIRE_SIZE = 5;

/** Number of values in the engine's binary snapshot. */
export const SNAPSHOT_WIRE_LENGTH = 10 + WIRE_TRACK_IDS.length * 3;

/**
 * Decodes the engine's binary snapshot.
 *
 * The layout is `[seed high, seed low, sample rate, bpm, bar, beat, section,
 * energy, intensity, playing]`, then `[muted, solo, level]` for each track.
 */
export function decodeSnapshot(wire: Float64Array): AlgoWasmSnapshot {
  const seed = (BigInt(wire[0] ?? 0) << 32n) | BigInt(wire[1] ?? 0);

  return {
    seed: seed.toString(),
    sampleRate: wire[2] ?? 0,
    bpm: wire[3] ?? 0,
    currentBar: wire[4] ?? 0,
    currentBeat: wire[5] ?? 1,
    currentSection: WIRE_SECTIONS[wire[6] ?? 0] ?? 'intro',
    energy: wire[7] ?? 0,
    intensity: wire[8] ?? 0,
    isPlaying: wire[9] === 1,
    tracks: WIRE_TRACK_IDS.map((id, index) => ({
      id,
      muted: wire[10 + index * 3] === 1,
      solo: wire[11 + index * 3] === 1,
      level: wire[12 + index * 3] ?? 0
    }))
  };
}

/**
 * Decodes the engine's binary event buffer, five values per event:
 * `[kind, sample, a, b, c]`. Kind 0 is note-on, 3 section-change, 4 bar, and
 * 5 beat. Unknown kinds are skipped.
 */
export function decodeEvents(wire: Float64Array): AlgoWasmGeneratedEvent[] {
  const events: AlgoWasmGeneratedEvent[] = [];

  for (let offset = 0; offset + EVENT_WIRE_SIZE <= wire.length; offset += EVENT_WIRE_SIZE) {
    const sample = wire[offset + 1] ?? 0;
    const a = wire[offset + 2] ?? 0;
    const b = wire[offset + 3] ?? 0;
    const c = wire[offset + 4] ?? 0;

    switch (wire[offset]) {
      case 0:
        events.push({
          type: 'note-on',
          sample,
          track: WIRE_TRACK_IDS[a] ?? 'kick',
          note: b,
          velocity: c
        });
        break;
      case 3:
        events.push({
          type: 'section-change',
          sample,
          section: WIRE_SECTIONS[a] ?? 'intro',
          bar: b
        });
        break;
      case 4:
        events.push({ type: 'bar', sample, bar: a });
        break;
      case 5:
        events.push({ type: 'beat', sample, bar: a, beat: b });
        break;
      default:
        break;
    }
  }

  return events;
}
