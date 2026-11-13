import { AlgoWasmError } from './algo-wasm-error.js';
import { AlgoWasmEventEmitter } from './events.js';
import { AlgoWasmEngineBinding, instantiateAlgoWasm, moodIndex, trackIndex } from './load-wasm.js';
import { builtInProfiles, normaliseSeed, splitSeed, validateProfile } from './profile.js';
import { encodeProfile } from './profile-wire.js';
import { decodeEvents, decodeSnapshot } from './wire-decode.js';
import type {
  AlgoWasmEventHandler,
  AlgoWasmEventName,
  AlgoWasmGeneratedEvent,
  AlgoWasmMidiExportOptions,
  AlgoWasmMood,
  AlgoWasmOfflineRenderOptions,
  AlgoWasmPlayerOptions,
  AlgoWasmProfile,
  AlgoWasmSnapshot,
  AlgoWasmTrackId
} from './types.js';

type PendingCommand = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

type WorkletMessage =
  | {
      type: 'ready';
      snapshot: Float64Array;
    }
  | {
      type: 'command-result';
      id: number;
      ok: boolean;
      snapshot?: Float64Array;
      result?: ArrayBuffer;
      error?: string;
    }
  | {
      type: 'events';
      events: Float64Array;
      snapshot: Float64Array;
    }
  | {
      type: 'error';
      error: string;
    };

/** Longest offline render accepted, in seconds. */
const MAX_OFFLINE_SECONDS = 600;

/** Frames rendered per chunk offline, before checking whether to yield. */
const OFFLINE_CHUNK_FRAMES = 8192;

/**
 * Plays generated music through an `AudioWorklet`.
 *
 * Create a player with `AlgoWasmPlayer.create`, call `start` from a user
 * gesture, and call `destroy` when finished. `destroy` frees the engine's
 * WASM memory and, if the player created its own `AudioContext`, closes it.
 *
 * ```ts
 * const player = await AlgoWasmPlayer.create({ wasmUrl, workletUrl, seed: 1995n });
 * button.addEventListener('click', () => player.start());
 * // Later:
 * await player.destroy();
 * ```
 */
export class AlgoWasmPlayer {
  private readonly emitter = new AlgoWasmEventEmitter();
  private readonly audioContext: AudioContext;
  private readonly ownsAudioContext: boolean;
  private readonly node: AudioWorkletNode;
  private readonly wasmUrl: string;
  private readonly pending = new Map<number, PendingCommand>();
  private commandId = 1;
  private destroyed = false;
  private destroyPromise?: Promise<void>;
  private offlineModule?: Promise<WebAssembly.Module>;
  private profile: AlgoWasmProfile;
  private seed: bigint;
  private lastSnapshot: AlgoWasmSnapshot;
  private lastEvents: AlgoWasmGeneratedEvent[] = [];
  // The live control state is mirrored here so that offline renders can
  // apply the same settings as the playing engine.
  private volume = 0.8;
  private energy = 0.5;
  private intensity = 0.5;
  private mood: AlgoWasmMood = 'dark';
  private tempoRange?: { minBpm: number; maxBpm: number };
  private readonly muted = new Set<AlgoWasmTrackId>();
  private readonly solo = new Set<AlgoWasmTrackId>();

  private constructor(
    audioContext: AudioContext,
    ownsAudioContext: boolean,
    node: AudioWorkletNode,
    wasmUrl: string,
    profile: AlgoWasmProfile,
    seed: bigint
  ) {
    this.audioContext = audioContext;
    this.ownsAudioContext = ownsAudioContext;
    this.node = node;
    this.wasmUrl = wasmUrl;
    this.profile = profile;
    this.seed = seed;
    this.lastSnapshot = createInitialSnapshot(profile, seed, audioContext.sampleRate);
    this.node.port.onmessage = (event: MessageEvent<WorkletMessage>) => this.handleMessage(event.data);
  }

  /**
   * Loads the worklet and the WASM engine and returns a ready player.
   *
   * No sound plays until `start` is called. If loading fails, everything
   * created along the way is released before the error is thrown.
   *
   * @throws {TypeError} When `wasmUrl` or `workletUrl` is missing.
   * @throws {AlgoWasmError} With code `browser_feature_missing`,
   * `profile_invalid`, `worklet_load_failed`, `wasm_load_failed`, or
   * `engine_failed`.
   */
  static async create(options: AlgoWasmPlayerOptions): Promise<AlgoWasmPlayer> {
    assertBrowserSupport();

    if (!options?.wasmUrl || !options.workletUrl) {
      throw new TypeError('Expected options.wasmUrl and options.workletUrl to be a string or URL.');
    }

    const profile = validateProfile(options.profile ?? builtInProfiles.amigaHouse95ish());
    const seed = normaliseSeed(options.seed);
    const wasmUrl = options.wasmUrl.toString();
    const workletUrl = options.workletUrl.toString();
    const ownsAudioContext = !options.audioContext;
    const audioContext = options.audioContext ?? new AudioContext();
    let node: AudioWorkletNode | undefined;

    try {
      if (!audioContext.audioWorklet) {
        throw new AlgoWasmError(
          'browser_feature_missing',
          'AudioWorklet is not available in this browser. AlgoWASM targets modern browsers from 2025 onwards.'
        );
      }

      try {
        await audioContext.audioWorklet.addModule(workletUrl);
      } catch (error) {
        throw new AlgoWasmError('worklet_load_failed', `Could not load the AlgoWASM worklet from ${workletUrl}.`, {
          cause: error
        });
      }

      node = new AudioWorkletNode(audioContext, 'algo-wasm-processor', {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [2]
      });
      node.connect(audioContext.destination);

      const player = new AlgoWasmPlayer(audioContext, ownsAudioContext, node, wasmUrl, profile, seed);
      player.volume = clampUnit(options.volume ?? 0.8);
      await player.configure();
      return player;
    } catch (error) {
      // Without this clean-up a failed load would leave a connected node and
      // an open AudioContext behind. Browsers only allow a handful of
      // contexts, so repeated retries would eventually stop audio working.
      if (node) {
        node.port.postMessage({ type: 'destroy' });
        node.disconnect();
        node.port.close();
      }

      if (ownsAudioContext && audioContext.state !== 'closed') {
        await audioContext.close().catch(() => {
          // The context may already be closing. The original error matters more.
        });
      }

      throw error;
    }
  }

  /**
   * Starts playback. Call this from a user gesture such as a click, because
   * browsers keep audio locked until then.
   *
   * @throws {AlgoWasmError} With code `audio_context_locked`, `engine_failed`,
   * or `destroyed`.
   */
  async start(): Promise<void> {
    this.assertAlive();

    try {
      const unlockState = await resumeAudioContext(this.audioContext);

      if (unlockState === 'pending') {
        this.emitter.emit(
          'warning',
          'AudioContext did not report a running state yet. Playback will continue when the browser unlocks audio.'
        );
      }
    } catch (error) {
      throw new AlgoWasmError(
        'audio_context_locked',
        'AudioContext could not resume. Call start() from a user gesture such as a click or tap.',
        { cause: error }
      );
    }

    await this.sendCommand('start');
    this.emitter.emit('started', this.lastSnapshot);
  }

  /** Pauses playback, keeping the position in the song. */
  async pause(): Promise<void> {
    this.assertAlive();
    await this.sendCommand('pause');
    this.emitter.emit('paused', this.lastSnapshot);
  }

  /** Resumes playback after `pause`. */
  async resume(): Promise<void> {
    this.assertAlive();
    const unlockState = await resumeAudioContext(this.audioContext);

    if (unlockState === 'pending') {
      this.emitter.emit(
        'warning',
        'AudioContext did not report a running state yet. Playback will continue when the browser unlocks audio.'
      );
    }

    await this.sendCommand('resume');
    this.emitter.emit('resumed', this.lastSnapshot);
  }

  /** Stops playback and rewinds the song to its first bar. */
  async stop(): Promise<void> {
    this.assertAlive();
    await this.sendCommand('stop');
    this.emitter.emit('stopped', this.lastSnapshot);
  }

  /**
   * Frees the engine and its WASM memory, disconnects the audio node, removes
   * every event handler, and closes the `AudioContext` if the player created
   * it. Safe to call more than once; later calls return the same promise.
   */
  async destroy(): Promise<void> {
    // Memoise the in-flight promise (set synchronously, before any await) so
    // overlapping calls share one teardown instead of each racing to send
    // their own "destroy" command and close the same AudioContext twice.
    if (this.destroyPromise) {
      return this.destroyPromise;
    }

    this.destroyPromise = this.performDestroy();
    return this.destroyPromise;
  }

  private async performDestroy(): Promise<void> {
    try {
      await this.sendBestEffort('destroy');
    } finally {
      this.destroyed = true;

      for (const command of this.pending.values()) {
        clearTimeout(command.timeout);
        command.reject(new AlgoWasmError('destroyed', 'AlgoWASM player was destroyed.'));
      }

      this.pending.clear();
      this.node.disconnect();
      this.node.port.close();
      this.emitter.clear();

      if (this.ownsAudioContext && this.audioContext.state !== 'closed') {
        try {
          await this.audioContext.close();
        } catch {
          // The browser may already be closing this context. Destruction must
          // still resolve rather than reject.
        }
      }
    }
  }

  /**
   * Changes the seed, which selects a different song. The new song starts
   * from its first bar, and keeps playing if the player was playing.
   *
   * @throws {AlgoWasmError} With code `profile_invalid` for a malformed seed.
   */
  setSeed(seed: bigint | number | string): void {
    this.seed = normaliseSeed(seed);
    const { high, low } = splitSeed(this.seed);
    this.postLiveCommand('set-seed', { seedHigh: high, seedLow: low });
  }

  /**
   * Changes the profile. The song restarts from its first bar, and keeps
   * playing if the player was playing.
   *
   * @throws {AlgoWasmError} With code `profile_invalid`.
   */
  setProfile(profile: AlgoWasmProfile): void {
    const validated = validateProfile(profile);

    // The engine can still refuse a valid profile, for example when its
    // sample rate limits exclude this AudioContext. The player only adopts
    // the profile once the engine has, so offline renders match playback.
    this.postLiveCommand('set-profile', { profile: encodeProfile(validated) }, () => {
      this.profile = validated;
      // A profile carries its own tempo range, which replaces any override.
      this.tempoRange = undefined;
    });
  }

  /** Sets master volume. Values are clamped to 0 to 1. */
  setVolume(volume: number): void {
    this.volume = clampUnit(volume);
    this.postLiveCommand('set-volume', { value: this.volume });
  }

  /**
   * Mutes or unmutes a track.
   *
   * @throws {TypeError} When the track id is not recognised.
   */
  setMuted(trackId: AlgoWasmTrackId, muted: boolean): void {
    const track = trackIndex(trackId);
    updateSet(this.muted, trackId, muted);
    this.postLiveCommand('set-muted', { track, flag: Boolean(muted) });
  }

  /**
   * Solos or unsolos a track. While any track is soloed, only soloed tracks
   * are heard.
   *
   * @throws {TypeError} When the track id is not recognised.
   */
  setSolo(trackId: AlgoWasmTrackId, solo: boolean): void {
    const track = trackIndex(trackId);
    updateSet(this.solo, trackId, solo);
    this.postLiveCommand('set-solo', { track, flag: Boolean(solo) });
  }

  /** Sets energy, which drives arrangement density. Clamped to 0 to 1. */
  setEnergy(value: number): void {
    this.energy = clampUnit(value);
    this.postLiveCommand('set-energy', { value: this.energy });
  }

  /** Sets intensity, which drives overall level. Clamped to 0 to 1. */
  setIntensity(value: number): void {
    this.intensity = clampUnit(value);
    this.postLiveCommand('set-intensity', { value: this.intensity });
  }

  /**
   * Sets the mood, which tilts the tone brighter or darker.
   *
   * @throws {TypeError} When the mood is not recognised.
   */
  setMood(mood: AlgoWasmMood): void {
    const index = moodIndex(mood);
    this.mood = mood;
    this.postLiveCommand('set-mood', { mood: index });
  }

  /**
   * Narrows or moves the tempo range. The song keeps its relative place in
   * the range. The range must sit inside the profile's `limits`; if it does
   * not, an `error` event is emitted and the tempo is unchanged.
   *
   * @throws {TypeError} When either value is not a finite number.
   * @throws {RangeError} When `minBpm` is greater than `maxBpm`.
   */
  setTempoRange(minBpm: number, maxBpm: number): void {
    if (!Number.isFinite(minBpm) || !Number.isFinite(maxBpm)) {
      throw new TypeError('Expected minBpm and maxBpm to be finite numbers.');
    }

    if (minBpm > maxBpm) {
      throw new RangeError('Expected minBpm to be less than or equal to maxBpm.');
    }

    // Remembered only once the engine accepts it. A range outside the
    // profile's limits must not be replayed into later offline renders.
    this.postLiveCommand('set-tempo-range', { minBpm, maxBpm }, () => {
      this.tempoRange = { minBpm, maxBpm };
    });
  }

  /** Returns a copy of the most recent engine state. */
  getSnapshot(): AlgoWasmSnapshot {
    return structuredClone(this.lastSnapshot);
  }

  /** Returns a copy of the most recent batch of generated events. */
  getCurrentEvents(): AlgoWasmGeneratedEvent[] {
    return structuredClone(this.lastEvents);
  }

  /**
   * Exports the current four-bar phrase as a type-0 MIDI file.
   *
   * When `options.downloadName` is given and a document is available, the
   * file is also offered to the user as a download with that name.
   */
  async exportMidi(options: AlgoWasmMidiExportOptions = {}): Promise<Uint8Array> {
    this.assertAlive();
    const bytes = new Uint8Array((await this.sendCommand('export-midi')) as ArrayBuffer);

    if (options.downloadName && typeof document !== 'undefined') {
      const url = URL.createObjectURL(new Blob([bytes], { type: 'audio/midi' }));
      const link = document.createElement('a');
      link.href = url;
      link.download = options.downloadName;
      link.click();
      URL.revokeObjectURL(url);
    }

    return bytes;
  }

  /**
   * Renders the song from its first bar into interleaved stereo samples
   * (left, right, left, ...), without playing it.
   *
   * The render uses the player's current volume, energy, intensity, mood,
   * tempo range, mutes, and solos. It runs in chunks and yields to the page
   * between them, so long renders do not freeze the tab.
   *
   * @throws {RangeError} When `seconds` is not between 0 and 600, or
   * `sampleRate` is not a whole number from 22050 to 96000.
   * @throws {AlgoWasmError} With code `profile_invalid`, `wasm_load_failed`,
   * `engine_failed`, or `destroyed`.
   */
  async renderOffline(options: AlgoWasmOfflineRenderOptions): Promise<Float32Array> {
    this.assertAlive();
    const seconds = options?.seconds;
    const sampleRate = options?.sampleRate ?? this.audioContext.sampleRate;

    if (typeof seconds !== 'number' || !(seconds >= 0 && seconds <= MAX_OFFLINE_SECONDS)) {
      throw new RangeError(`Expected seconds to be a number from 0 to ${MAX_OFFLINE_SECONDS}.`);
    }

    if (!Number.isInteger(sampleRate) || sampleRate < 22050 || sampleRate > 96000) {
      throw new RangeError('Expected sampleRate to be a whole number from 22050 to 96000.');
    }

    const profile = validateProfile(options.profile ?? this.profile);
    const seed = normaliseSeed(options.seed ?? this.seed);
    const output = new Float32Array(Math.floor(seconds * sampleRate) * 2);
    const instance = await WebAssembly.instantiate(await this.compileOffline(), {});
    const binding = new AlgoWasmEngineBinding(instance);

    try {
      binding.create(sampleRate, seed, profile);
      binding.setVolume(this.volume);
      binding.setEnergy(this.energy);
      binding.setIntensity(this.intensity);
      binding.setMood(this.mood);

      // An override only applies to the profile it was set against.
      if (this.tempoRange && !options.profile) {
        binding.setTempoRange(this.tempoRange.minBpm, this.tempoRange.maxBpm);
      }

      for (const trackId of this.muted) {
        binding.setMuted(trackId, true);
      }

      for (const trackId of this.solo) {
        binding.setSolo(trackId, true);
      }

      binding.start();
      let lastYield = performance.now();

      for (let offset = 0; offset < output.length; offset += OFFLINE_CHUNK_FRAMES * 2) {
        binding.renderTo(output.subarray(offset, offset + OFFLINE_CHUNK_FRAMES * 2));

        // Hand control back to the page every few milliseconds so input and
        // rendering stay responsive during a long render.
        if (performance.now() - lastYield > 12) {
          await new Promise(resolve => setTimeout(resolve));
          lastYield = performance.now();
        }
      }
    } finally {
      binding.destroy();
    }

    return output;
  }

  /**
   * Subscribes to a player event and returns a function that unsubscribes.
   *
   * `beat`, `bar`, and `section-change` arrive in batches about every 21 ms,
   * so they can trail the audio by that much.
   */
  on<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): () => void {
    return this.emitter.on(eventName, handler);
  }

  /** Removes a handler added with `on`. */
  off<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): void {
    this.emitter.off(eventName, handler);
  }

  /**
   * Compiles the WASM module for offline rendering on first use and reuses
   * it afterwards, so repeated renders do not fetch or compile again.
   */
  private compileOffline(): Promise<WebAssembly.Module> {
    this.offlineModule ??= instantiateAlgoWasm(this.wasmUrl).then(
      result => result.module,
      error => {
        // Forget a failed attempt so that the next call can try again.
        this.offlineModule = undefined;
        throw error;
      }
    );

    return this.offlineModule;
  }

  private async configure(): Promise<void> {
    const wasmBytes = await fetchWasmBytes(this.wasmUrl);
    const ready = new Promise<void>((resolve, reject) => {
      let offReady = () => {};
      let offError = () => {};
      const timeout = setTimeout(() => {
        offReady();
        offError();
        reject(
          new AlgoWasmError(
            'engine_failed',
            'AlgoWASM worklet did not become ready within five seconds.'
          )
        );
      }, 5000);

      offReady = this.on('ready', () => {
        clearTimeout(timeout);
        offReady();
        offError();
        resolve();
      });
      offError = this.on('error', error => {
        clearTimeout(timeout);
        offReady();
        offError();
        reject(error);
      });
    });
    const { high, low } = splitSeed(this.seed);

    this.node.port.postMessage(
      {
        type: 'configure',
        wasmBytes,
        profile: encodeProfile(this.profile),
        seedHigh: high,
        seedLow: low,
        value: this.volume,
        sampleRate: this.audioContext.sampleRate
      },
      [wasmBytes]
    );

    await ready;
  }

  /**
   * Sends a command without waiting for it. A failure is reported through
   * the `error` event. `onAccepted` runs only if the engine accepts it.
   */
  private postLiveCommand(
    type: string,
    payload: Record<string, unknown>,
    onAccepted?: () => void
  ): void {
    if (this.destroyed) {
      throw new AlgoWasmError('destroyed', 'AlgoWASM player has been destroyed.');
    }

    void this.sendCommand(type, payload).then(onAccepted).catch(error => {
      this.emitter.emit('error', error instanceof Error ? error : new Error(String(error)));
    });
  }

  private async sendBestEffort(type: string, payload: Record<string, unknown> = {}): Promise<void> {
    try {
      await this.sendCommand(type, payload, 1000);
    } catch {
      // Destruction must finish even if the worklet has already gone away.
    }
  }

  private sendCommand(
    type: string,
    payload: Record<string, unknown> = {},
    timeoutMs = 5000
  ): Promise<unknown> {
    this.assertAlive();
    const id = this.commandId++;

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new AlgoWasmError('engine_failed', `Worklet command ${type} timed out.`));
      }, timeoutMs);

      this.pending.set(id, { resolve, reject, timeout });
      this.node.port.postMessage({
        type,
        id,
        ...payload
      });
    });
  }

  private handleMessage(message: WorkletMessage): void {
    if (message.type === 'ready') {
      this.lastSnapshot = decodeSnapshot(message.snapshot);
      this.emitter.emit('ready', this.lastSnapshot);
      return;
    }

    if (message.type === 'events') {
      this.lastEvents = decodeEvents(message.events);
      this.lastSnapshot = decodeSnapshot(message.snapshot);

      for (const event of this.lastEvents) {
        if (event.type === 'section-change' || event.type === 'bar' || event.type === 'beat') {
          this.emitter.emit(event.type, event);
        }
      }

      return;
    }

    if (message.type === 'error') {
      this.emitter.emit('error', new AlgoWasmError('engine_failed', message.error));
      return;
    }

    const pending = this.pending.get(message.id);

    if (!pending) {
      return;
    }

    clearTimeout(pending.timeout);
    this.pending.delete(message.id);

    if (message.ok) {
      // The snapshot is applied before the promise resolves, so callers that
      // await a command always read the state that command produced.
      if (message.snapshot) {
        this.lastSnapshot = decodeSnapshot(message.snapshot);
      }

      pending.resolve(message.result);
      return;
    }

    pending.reject(new AlgoWasmError('engine_failed', message.error ?? 'Worklet command failed.'));
  }

  private assertAlive(): void {
    if (this.destroyed) {
      throw new AlgoWasmError('destroyed', 'AlgoWASM player has been destroyed.');
    }
  }
}

function updateSet<Value>(set: Set<Value>, value: Value, present: boolean): void {
  if (present) {
    set.add(value);
  } else {
    set.delete(value);
  }
}

function assertBrowserSupport(): void {
  if (typeof AudioContext === 'undefined') {
    throw new AlgoWasmError(
      'browser_feature_missing',
      'AudioContext is not available in this browser. AlgoWASM targets modern browsers from 2025 onwards.'
    );
  }

  if (typeof WebAssembly === 'undefined') {
    throw new AlgoWasmError(
      'browser_feature_missing',
      'WebAssembly is not available in this browser. AlgoWASM cannot run here.'
    );
  }
}

function createInitialSnapshot(
  profile: AlgoWasmProfile,
  seed: bigint,
  sampleRate: number
): AlgoWasmSnapshot {
  return {
    seed: seed.toString(),
    sampleRate,
    bpm: profile.tempo.defaultBpm,
    currentBar: 0,
    currentBeat: 1,
    currentSection: 'intro',
    energy: 0.5,
    intensity: 0.5,
    isPlaying: false,
    tracks: profile.mixer.tracks.map(track => ({
      id: track.track,
      muted: false,
      solo: false,
      level: track.gain
    }))
  };
}

function clampUnit(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }

  return Math.min(1, Math.max(0, value));
}

async function resumeAudioContext(audioContext: AudioContext): Promise<'running' | 'pending'> {
  if (audioContext.state === 'running') {
    return 'running';
  }

  return Promise.race([
    audioContext.resume().then(() => 'running' as const),
    new Promise<'pending'>(resolve => {
      setTimeout(() => resolve('pending'), 1200);
    })
  ]);
}

async function fetchWasmBytes(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);

  if (!response.ok) {
    throw new AlgoWasmError(
      'wasm_load_failed',
      `Could not load AlgoWASM from ${url}. The server returned ${response.status}.`
    );
  }

  return response.arrayBuffer();
}
