import { AlgoWasmError } from './AlgoWasmError.js';
import { EVENT_WIRE_SIZE, SNAPSHOT_WIRE_LENGTH } from './wire-decode.js';

type WasmFn = (...args: number[]) => number;

/**
 * Low-level, numbers-only binding to one engine inside a WASM instance.
 *
 * Both the audio worklet and the main thread use this class, so the boundary
 * code exists once. Nothing here allocates per call once the buffers exist,
 * apart from the copies returned by `copySnapshot`, `copyEvents`, and
 * `exportMidi`, which makes it safe to use on the audio thread.
 *
 * Memory ownership: the binding owns every WASM buffer it allocates and
 * frees them in `destroy`. Call `destroy` when finished.
 */
export class EngineCore {
  private readonly exports: Record<string, WebAssembly.ExportValue>;
  private enginePtr = 0;
  private renderPtr = 0;
  private renderLength = 0;
  private renderView?: Float32Array;
  private failed = false;

  /**
   * @throws {AlgoWasmError} With code `wasm_load_failed` when the instance
   * does not export memory, which means it is not an AlgoWASM binary.
   */
  constructor(instance: WebAssembly.Instance) {
    this.exports = instance.exports as Record<string, WebAssembly.ExportValue>;

    if (!(this.exports.memory instanceof WebAssembly.Memory)) {
      throw new AlgoWasmError('wasm_load_failed', 'AlgoWASM did not export a WebAssembly memory.');
    }
  }

  /**
   * Creates the engine from a profile in the flat layout written by
   * `encodeProfile`. Any existing engine is destroyed first.
   *
   * @throws {AlgoWasmError} With code `engine_failed` and the engine's own
   * message when the profile or sample rate is rejected.
   */
  createFromWire(sampleRate: number, seedHigh: number, seedLow: number, profile: Float32Array): void {
    this.destroy();
    const pointer = this.writeProfile(profile);

    try {
      const result = this.call(
        'algo_wasm_engine_create',
        sampleRate,
        seedHigh,
        seedLow,
        pointer,
        profile.length
      );

      if (result === 0) {
        throw new AlgoWasmError('engine_failed', this.lastError('AlgoWASM engine creation failed.'));
      }

      this.enginePtr = result;
    } finally {
      this.call('algo_wasm_dealloc', pointer, profile.byteLength);
    }
  }

  /** Starts playback. */
  start(): void {
    this.callEngine('algo_wasm_engine_start');
  }

  /** Pauses playback, keeping the transport position. */
  pause(): void {
    this.callEngine('algo_wasm_engine_pause');
  }

  /** Resumes playback after a pause. */
  resume(): void {
    this.callEngine('algo_wasm_engine_resume');
  }

  /** Stops playback, rewinds to the first bar, and clears every voice. */
  stop(): void {
    this.callEngine('algo_wasm_engine_stop');
  }

  /** Re-seeds the engine from the two halves of a 64-bit seed. */
  setSeedParts(seedHigh: number, seedLow: number): void {
    this.callEngine('algo_wasm_engine_set_seed', seedHigh, seedLow);
  }

  /** Replaces the profile, given in the flat layout, and restarts the song. */
  setProfileWire(profile: Float32Array): void {
    this.ensureEngine();
    const pointer = this.writeProfile(profile);

    try {
      this.assertCode(
        this.call('algo_wasm_engine_set_profile', this.enginePtr, pointer, profile.length),
        'Profile update failed.'
      );
    } finally {
      this.call('algo_wasm_dealloc', pointer, profile.byteLength);
    }
  }

  /** Sets master volume from 0 to 1. */
  setVolume(value: number): void {
    this.callEngine('algo_wasm_engine_set_volume', value);
  }

  /** Sets energy from 0 to 1. */
  setEnergy(value: number): void {
    this.callEngine('algo_wasm_engine_set_energy', value);
  }

  /** Sets intensity from 0 to 1. */
  setIntensity(value: number): void {
    this.callEngine('algo_wasm_engine_set_intensity', value);
  }

  /** Sets mood by its index in `WIRE_MOODS`. */
  setMoodIndex(mood: number): void {
    this.callEngine('algo_wasm_engine_set_mood', mood);
  }

  /** Sets the tempo range in beats per minute. */
  setTempoRange(minBpm: number, maxBpm: number): void {
    this.callEngine('algo_wasm_engine_set_tempo_range', minBpm, maxBpm);
  }

  /** Mutes or unmutes a track by its index in `WIRE_TRACK_IDS`. */
  setMutedIndex(track: number, muted: boolean): void {
    this.callEngine('algo_wasm_engine_set_muted', track, muted ? 1 : 0);
  }

  /** Solos or unsolos a track by its index in `WIRE_TRACK_IDS`. */
  setSoloIndex(track: number, solo: boolean): void {
    this.callEngine('algo_wasm_engine_set_solo', track, solo ? 1 : 0);
  }

  /** Renders interleaved stereo samples (left, right, left, ...) into `output`. */
  renderTo(output: Float32Array): void {
    output.set(this.render(output.length).subarray(0, output.length));
  }

  /**
   * Renders straight into separate left and right channel buffers, as an
   * audio worklet receives them. The two must be the same length.
   */
  renderPlanar(left: Float32Array, right: Float32Array): void {
    const view = this.render(left.length * 2);

    for (let index = 0; index < left.length; index += 1) {
      left[index] = view[index * 2] ?? 0;
      right[index] = view[index * 2 + 1] ?? 0;
    }
  }

  /** Returns a copy of the engine's binary snapshot. See `decodeSnapshot`. */
  copySnapshot(): Float64Array {
    this.ensureEngine();
    const pointer = this.call('algo_wasm_engine_snapshot', this.enginePtr);

    if (pointer === 0) {
      throw new AlgoWasmError('engine_failed', this.lastError('Snapshot failed.'));
    }

    return new Float64Array(this.memory.buffer, pointer, SNAPSHOT_WIRE_LENGTH).slice();
  }

  /**
   * Removes the events buffered since the last call and returns a copy of
   * them in binary form, or `undefined` when there were none. See
   * `decodeEvents`.
   */
  copyEvents(): Float64Array | undefined {
    this.ensureEngine();
    const count = this.call('algo_wasm_engine_drain_events', this.enginePtr);

    if (count === 0) {
      return undefined;
    }

    const pointer = this.call('algo_wasm_engine_events_ptr', this.enginePtr);
    return new Float64Array(this.memory.buffer, pointer, count * EVENT_WIRE_SIZE).slice();
  }

  /** Returns the current four-bar phrase as the bytes of a type-0 MIDI file. */
  exportMidi(): Uint8Array {
    this.ensureEngine();
    const length = this.call('algo_wasm_engine_export_midi', this.enginePtr);
    const pointer = this.call('algo_wasm_engine_midi_ptr', this.enginePtr);

    return new Uint8Array(this.memory.buffer, pointer, length).slice();
  }

  /**
   * Frees the engine and every WASM buffer this binding allocated. Safe to
   * call more than once. After an internal engine error nothing is freed,
   * because the instance can no longer be trusted; drop the instance instead.
   */
  destroy(): void {
    if (this.failed) {
      this.enginePtr = 0;
      this.renderPtr = 0;
      return;
    }

    if (this.renderPtr !== 0) {
      this.call('algo_wasm_dealloc', this.renderPtr, this.renderLength * 4);
      this.renderPtr = 0;
      this.renderLength = 0;
      this.renderView = undefined;
    }

    if (this.enginePtr !== 0) {
      this.call('algo_wasm_engine_destroy', this.enginePtr);
      this.enginePtr = 0;
    }
  }

  private get memory(): WebAssembly.Memory {
    return this.exports.memory as WebAssembly.Memory;
  }

  /** Renders `length` interleaved samples and returns a view of them. */
  private render(length: number): Float32Array {
    this.ensureEngine();

    if (this.renderLength < length) {
      if (this.renderPtr !== 0) {
        this.call('algo_wasm_dealloc', this.renderPtr, this.renderLength * 4);
      }

      this.renderPtr = this.call('algo_wasm_alloc', length * 4);
      this.renderLength = this.renderPtr === 0 ? 0 : length;
      this.renderView = undefined;

      if (this.renderPtr === 0) {
        throw new AlgoWasmError('render_failed', 'Could not allocate the WASM render buffer.');
      }
    }

    this.assertCode(
      this.call('algo_wasm_engine_render', this.enginePtr, this.renderPtr, length),
      'Render failed.'
    );

    // WASM memory can grow, which detaches every existing view of it. The
    // view is only rebuilt when that has happened, so the audio thread does
    // not create a new object on each block.
    if (!this.renderView || this.renderView.buffer !== this.memory.buffer) {
      this.renderView = new Float32Array(this.memory.buffer, this.renderPtr, this.renderLength);
    }

    return this.renderView;
  }

  private writeProfile(profile: Float32Array): number {
    const pointer = this.call('algo_wasm_alloc', profile.byteLength);

    if (pointer === 0) {
      throw new AlgoWasmError('engine_failed', 'Could not allocate a WASM buffer for the profile.');
    }

    new Float32Array(this.memory.buffer, pointer, profile.length).set(profile);
    return pointer;
  }

  private callEngine(name: string, ...args: number[]): void {
    this.ensureEngine();
    this.assertCode(this.call(name, this.enginePtr, ...args), `${name} failed.`);
  }

  private assertCode(code: number, fallback: string): void {
    if (code !== 0) {
      throw new AlgoWasmError('engine_failed', this.lastError(fallback));
    }
  }

  private ensureEngine(): void {
    if (this.enginePtr === 0) {
      throw new AlgoWasmError('engine_failed', 'AlgoWASM engine has not been created.');
    }
  }

  private call(name: string, ...args: number[]): number {
    if (this.failed) {
      throw new AlgoWasmError(
        'engine_failed',
        'The AlgoWASM engine stopped after an internal error. Create a new player to continue.'
      );
    }

    const fn = this.exports[name];

    if (typeof fn !== 'function') {
      throw new AlgoWasmError('wasm_load_failed', `AlgoWASM is missing the ${name} export.`);
    }

    try {
      return (fn as WasmFn)(...args);
    } catch (error) {
      // A trap means the engine panicked part-way through a change. Its state
      // can no longer be trusted, so every later call is refused.
      this.failed = true;
      throw new AlgoWasmError(
        'engine_failed',
        'The AlgoWASM engine stopped after an internal error. Create a new player to continue.',
        { cause: error }
      );
    }
  }

  private lastError(fallback: string): string {
    const length = this.call('algo_wasm_last_error_len');

    if (length === 0) {
      return fallback;
    }

    const pointer = this.call('algo_wasm_last_error_ptr');
    // Engine messages are plain ASCII, so they are decoded directly. Audio
    // worklets do not provide TextDecoder.
    return String.fromCharCode(...new Uint8Array(this.memory.buffer, pointer, length));
  }
}
