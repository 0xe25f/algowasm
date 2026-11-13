import { AlgoWasmError } from './AlgoWasmError.js';
import { EngineCore } from './engine-core.js';
import { splitSeed, validateProfile } from './profile.js';
import { encodeProfile, WIRE_MOODS, WIRE_TRACK_IDS } from './profile-wire.js';
import { decodeEvents, decodeSnapshot } from './wire-decode.js';
import type {
  AlgoWasmGeneratedEvent,
  AlgoWasmMood,
  AlgoWasmProfile,
  AlgoWasmSnapshot,
  AlgoWasmTrackId
} from './types.js';

/** A compiled and instantiated AlgoWASM binary. */
export interface AlgoWasmInstantiation {
  instance: WebAssembly.Instance;
  module: WebAssembly.Module;
}

/**
 * Fetches and instantiates the AlgoWASM binary.
 *
 * Streaming compilation is used when the server sends `application/wasm`. If
 * the MIME type is wrong, as on some local servers, the file is fetched again
 * and compiled from bytes instead.
 *
 * @throws {AlgoWasmError} With code `wasm_load_failed` when the file cannot
 * be fetched or compiled.
 */
export async function instantiateAlgoWasm(wasmUrl: string | URL): Promise<AlgoWasmInstantiation> {
  const url = wasmUrl.toString();
  const imports: WebAssembly.Imports = {};
  const response = await fetch(url);

  if (!response.ok) {
    throw new AlgoWasmError(
      'wasm_load_failed',
      `Could not load AlgoWASM from ${url}. The server returned ${response.status}.`
    );
  }

  if (WebAssembly.instantiateStreaming) {
    try {
      const result = await WebAssembly.instantiateStreaming(response, imports);
      return {
        instance: result.instance,
        module: result.module
      };
    } catch (error) {
      if (response.headers.get('content-type') === 'application/wasm') {
        throw new AlgoWasmError(
          'wasm_load_failed',
          'WASM streaming compilation failed even though the server served application/wasm.',
          { cause: error }
        );
      }
    }
  }

  const fallbackResponse = await fetch(url);

  if (!fallbackResponse.ok) {
    throw new AlgoWasmError(
      'wasm_load_failed',
      `Could not load AlgoWASM fallback bytes from ${url}. The server returned ${fallbackResponse.status}.`
    );
  }

  const bytes = await fallbackResponse.arrayBuffer();
  const result = await WebAssembly.instantiate(bytes, imports);

  return {
    instance: result.instance,
    module: result.module
  };
}

/**
 * Converts a mood id to its engine index.
 *
 * @throws {TypeError} When the mood is not recognised.
 */
export function moodIndex(mood: AlgoWasmMood): number {
  const index = WIRE_MOODS.indexOf(mood);

  if (index < 0) {
    throw new TypeError(`Expected mood to be one of ${WIRE_MOODS.join(', ')}.`);
  }

  return index;
}

/**
 * Converts a track id to its engine index.
 *
 * @throws {TypeError} When the track id is not recognised.
 */
export function trackIndex(trackId: AlgoWasmTrackId): number {
  const index = WIRE_TRACK_IDS.indexOf(trackId);

  if (index < 0) {
    throw new TypeError(`Expected trackId to be one of ${WIRE_TRACK_IDS.join(', ')}.`);
  }

  return index;
}

/**
 * Typed binding to one engine inside a WASM instance, for use outside an
 * `AlgoWasmPlayer`, for example to render audio offline.
 *
 * Memory ownership: the binding owns the WASM buffers it allocates. Call
 * `destroy` when finished to free them.
 *
 * ```ts
 * const { instance } = await instantiateAlgoWasm(wasmUrl);
 * const binding = new AlgoWasmEngineBinding(instance);
 *
 * try {
 *   binding.create(48000, 42n, builtInProfiles.dubstep());
 *   binding.start();
 *   binding.renderTo(output);
 * } finally {
 *   binding.destroy();
 * }
 * ```
 */
export class AlgoWasmEngineBinding extends EngineCore {
  /**
   * Validates the profile and creates the engine.
   *
   * @throws {AlgoWasmError} With code `profile_invalid` or `engine_failed`.
   */
  create(sampleRate: number, seed: bigint, profile: AlgoWasmProfile): void {
    const { high, low } = splitSeed(seed);
    this.createFromWire(sampleRate, high, low, encodeProfile(validateProfile(profile)));
  }

  /** Re-seeds the engine and restarts the song. */
  setSeed(seed: bigint): void {
    const { high, low } = splitSeed(seed);
    this.setSeedParts(high, low);
  }

  /**
   * Validates and applies a new profile, then restarts the song.
   *
   * @throws {AlgoWasmError} With code `profile_invalid` or `engine_failed`.
   */
  setProfile(profile: AlgoWasmProfile): void {
    this.setProfileWire(encodeProfile(validateProfile(profile)));
  }

  /**
   * Sets the mood.
   *
   * @throws {TypeError} When the mood is not recognised.
   */
  setMood(value: AlgoWasmMood): void {
    this.setMoodIndex(moodIndex(value));
  }

  /**
   * Mutes or unmutes a track.
   *
   * @throws {TypeError} When the track id is not recognised.
   */
  setMuted(trackId: AlgoWasmTrackId, muted: boolean): void {
    this.setMutedIndex(trackIndex(trackId), muted);
  }

  /**
   * Solos or unsolos a track.
   *
   * @throws {TypeError} When the track id is not recognised.
   */
  setSolo(trackId: AlgoWasmTrackId, solo: boolean): void {
    this.setSoloIndex(trackIndex(trackId), solo);
  }

  /** Returns the engine's current state. */
  snapshot(): AlgoWasmSnapshot {
    return decodeSnapshot(this.copySnapshot());
  }

  /**
   * Returns the events generated since the last call, and removes them from
   * the engine's buffer. The buffer holds up to the profile's
   * `limits.maxEventsPerBlock` events, so call this at least that often.
   */
  events(): AlgoWasmGeneratedEvent[] {
    const wire = this.copyEvents();
    return wire ? decodeEvents(wire) : [];
  }
}
