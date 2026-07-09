import { AlgoWasmError } from './AlgoWasmError.js';
import { AlgoWasmEventEmitter } from './events.js';
import { AlgoWasmEngineBinding, instantiateAlgoWasm } from './loadWasm.js';
import { builtInProfiles, normaliseSeed, validateProfile } from './profile.js';
import type {
  AlgoWasmEventHandler,
  AlgoWasmEventName,
  AlgoWasmGeneratedEvent,
  AlgoWasmMetrics,
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
      snapshot: AlgoWasmSnapshot;
    }
  | {
      type: 'command-result';
      id: number;
      ok: boolean;
      result?: unknown;
      error?: string;
    }
  | {
      type: 'events';
      events: AlgoWasmGeneratedEvent[];
      snapshot?: AlgoWasmSnapshot;
      metrics?: AlgoWasmMetrics;
    }
  | {
      type: 'error';
      error: string;
    };

export class AlgoWasmPlayer {
  private readonly emitter = new AlgoWasmEventEmitter();
  private readonly audioContext: AudioContext;
  private readonly ownsAudioContext: boolean;
  private readonly node: AudioWorkletNode;
  private readonly wasmUrl: string;
  private readonly workletUrl: string;
  private readonly pending = new Map<number, PendingCommand>();
  private commandId = 1;
  private destroyed = false;
  private destroyPromise?: Promise<void>;
  private profile: AlgoWasmProfile;
  private seed: bigint;
  private lastSnapshot: AlgoWasmSnapshot;
  private lastEvents: AlgoWasmGeneratedEvent[] = [];

  private constructor(
    audioContext: AudioContext,
    ownsAudioContext: boolean,
    node: AudioWorkletNode,
    wasmUrl: string,
    workletUrl: string,
    profile: AlgoWasmProfile,
    seed: bigint,
    initialSnapshot: AlgoWasmSnapshot
  ) {
    this.audioContext = audioContext;
    this.ownsAudioContext = ownsAudioContext;
    this.node = node;
    this.wasmUrl = wasmUrl;
    this.workletUrl = workletUrl;
    this.profile = profile;
    this.seed = seed;
    this.lastSnapshot = initialSnapshot;
    this.node.port.onmessage = (event: MessageEvent<WorkletMessage>) => this.handleMessage(event.data);
  }

  static async create(options: AlgoWasmPlayerOptions): Promise<AlgoWasmPlayer> {
    assertBrowserSupport();

    const profile = validateProfile(options.profile ?? builtInProfiles.amigaHouse95ish());
    const seed = normaliseSeed(options.seed);
    const wasmUrl = options.wasmUrl.toString();
    const workletUrl = options.workletUrl.toString();
    const ownsAudioContext = !options.audioContext;
    const audioContext = options.audioContext ?? new AudioContext();

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

    const node = new AudioWorkletNode(audioContext, 'algo-wasm-processor', {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [2]
    });
    node.connect(audioContext.destination);

    const initialSnapshot = createInitialSnapshot(profile, seed, audioContext.sampleRate);
    const player = new AlgoWasmPlayer(
      audioContext,
      ownsAudioContext,
      node,
      wasmUrl,
      workletUrl,
      profile,
      seed,
      initialSnapshot
    );

    await player.configure(options.volume ?? 0.8);
    return player;
  }

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

    const snapshot = (await this.sendCommand('start')) as AlgoWasmSnapshot;
    this.updateSnapshot(snapshot);
    this.emitter.emit('started', this.lastSnapshot);
  }

  async pause(): Promise<void> {
    this.assertAlive();
    const snapshot = (await this.sendCommand('pause')) as AlgoWasmSnapshot;
    this.updateSnapshot(snapshot);
    this.emitter.emit('paused', this.lastSnapshot);
  }

  async resume(): Promise<void> {
    this.assertAlive();
    const unlockState = await resumeAudioContext(this.audioContext);

    if (unlockState === 'pending') {
      this.emitter.emit(
        'warning',
        'AudioContext did not report a running state yet. Playback will continue when the browser unlocks audio.'
      );
    }

    const snapshot = (await this.sendCommand('resume')) as AlgoWasmSnapshot;
    this.updateSnapshot(snapshot);
    this.emitter.emit('resumed', this.lastSnapshot);
  }

  async stop(): Promise<void> {
    this.assertAlive();
    const snapshot = (await this.sendCommand('stop')) as AlgoWasmSnapshot;
    this.updateSnapshot(snapshot);
    this.emitter.emit('stopped', this.lastSnapshot);
  }

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

  setSeed(seed: bigint | number | string): void {
    this.seed = normaliseSeed(seed);
    this.postLiveCommand('set-seed', { seed: this.seed.toString() });
  }

  setProfile(profile: AlgoWasmProfile): void {
    this.profile = validateProfile(profile);
    this.postLiveCommand('set-profile', { profile: this.profile });
  }

  setVolume(volume: number): void {
    this.postLiveCommand('set-volume', { value: clampUnit(volume) });
  }

  setMuted(trackId: AlgoWasmTrackId, muted: boolean): void {
    this.postLiveCommand('set-muted', { trackId, muted });
  }

  setSolo(trackId: AlgoWasmTrackId, solo: boolean): void {
    this.postLiveCommand('set-solo', { trackId, solo });
  }

  setEnergy(value: number): void {
    this.postLiveCommand('set-energy', { value: clampUnit(value) });
  }

  setIntensity(value: number): void {
    this.postLiveCommand('set-intensity', { value: clampUnit(value) });
  }

  setMood(mood: AlgoWasmMood): void {
    this.postLiveCommand('set-mood', { mood });
  }

  setTempoRange(minBpm: number, maxBpm: number): void {
    this.postLiveCommand('set-tempo-range', { minBpm, maxBpm });
  }

  getSnapshot(): AlgoWasmSnapshot {
    return structuredClone(this.lastSnapshot);
  }

  getCurrentEvents(): AlgoWasmGeneratedEvent[] {
    return structuredClone(this.lastEvents);
  }

  async exportMidi(_options: AlgoWasmMidiExportOptions = {}): Promise<Uint8Array> {
    this.assertAlive();
    const bytes = (await this.sendCommand('export-midi')) as ArrayBuffer;
    return new Uint8Array(bytes);
  }

  async renderOffline(options: AlgoWasmOfflineRenderOptions): Promise<Float32Array> {
    this.assertAlive();
    const profile = validateProfile(options.profile ?? this.profile);
    const seed = normaliseSeed(options.seed ?? this.seed);
    const sampleRate = options.sampleRate ?? this.audioContext.sampleRate;
    const frames = Math.max(0, Math.floor(options.seconds * sampleRate));
    const output = new Float32Array(frames * 2);
    const { instance } = await instantiateAlgoWasm(this.wasmUrl);
    const binding = new AlgoWasmEngineBinding(instance);

    try {
      binding.create(sampleRate, seed, profile);
      binding.start();
      const chunkFrames = 2048;
      let offset = 0;

      while (offset < output.length) {
        const chunkLength = Math.min(chunkFrames * 2, output.length - offset);
        const chunk = new Float32Array(chunkLength);
        binding.renderTo(chunk);
        output.set(chunk, offset);
        offset += chunkLength;
      }
    } finally {
      binding.destroy();
    }

    return output;
  }

  on<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): () => void {
    return this.emitter.on(eventName, handler);
  }

  off<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): void {
    this.emitter.off(eventName, handler);
  }

  private async configure(volume: number): Promise<void> {
    const wasmBytes = await fetchWasmBytes(this.wasmUrl);
    const ready = new Promise<AlgoWasmSnapshot>((resolve, reject) => {
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

      offReady = this.on('ready', snapshot => {
        clearTimeout(timeout);
        offReady();
        offError();
        resolve(snapshot);
      });
      offError = this.on('error', error => {
        clearTimeout(timeout);
        offReady();
        offError();
        reject(error);
      });
    });

    this.node.port.postMessage(
      {
        type: 'configure',
        wasmUrl: this.wasmUrl,
        wasmBytes,
        profile: this.profile,
        seed: this.seed.toString(),
        volume: clampUnit(volume),
        sampleRate: this.audioContext.sampleRate
      },
      [wasmBytes]
    );

    this.updateSnapshot(await ready);
  }

  private postLiveCommand(type: string, payload: Record<string, unknown>): void {
    if (this.destroyed) {
      throw new AlgoWasmError('destroyed', 'AlgoWASM player has been destroyed.');
    }

    void this.sendCommand(type, payload).catch(error => {
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
      this.updateSnapshot(message.snapshot);
      this.emitter.emit('ready', this.lastSnapshot);
      return;
    }

    if (message.type === 'events') {
      this.lastEvents = message.events;

      if (message.snapshot) {
        this.updateSnapshot(message.snapshot);
      }

      if (message.metrics) {
        this.emitter.emit('metrics', message.metrics);
      }

      for (const event of message.events) {
        if (event.type === 'section-change') {
          this.emitter.emit('section-change', event);
        } else if (event.type === 'bar') {
          this.emitter.emit('bar', event);
        } else if (event.type === 'beat') {
          this.emitter.emit('beat', event);
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
      pending.resolve(message.result);
      return;
    }

    pending.reject(new AlgoWasmError('engine_failed', message.error ?? 'Worklet command failed.'));
  }

  private updateSnapshot(snapshot: AlgoWasmSnapshot): void {
    this.lastSnapshot = structuredClone(snapshot);
  }

  private assertAlive(): void {
    if (this.destroyed) {
      throw new AlgoWasmError('destroyed', 'AlgoWASM player has been destroyed.');
    }
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
