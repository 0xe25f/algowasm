import { EngineCore } from '../engine-core.js';

declare const sampleRate: number;

declare class AudioWorkletProcessor {
  readonly port: MessagePort;
  process(
    inputs: Float32Array[][],
    outputs: Float32Array[][],
    parameters: Record<string, Float32Array>
  ): boolean;
}

declare function registerProcessor(
  name: string,
  processorCtor: typeof AudioWorkletProcessor
): void;

/**
 * A message from the player. Every value is already validated and numeric:
 * the main thread turns ids into indexes and profiles into flat buffers, so
 * the audio thread never parses text.
 */
type WorkletCommand = {
  type: string;
  id?: number;
  wasmBytes?: ArrayBuffer;
  profile?: Float32Array;
  seedHigh?: number;
  seedLow?: number;
  sampleRate?: number;
  value?: number;
  track?: number;
  flag?: boolean;
  mood?: number;
  minBpm?: number;
  maxBpm?: number;
};

/** Frames rendered between checks for new engine events, about 21 ms at 48 kHz. */
const EVENT_POLL_FRAMES = 1024;

/**
 * Renders the engine on the audio thread and relays commands and events.
 *
 * The WASM binding is shared with the main thread through `EngineCore`; the
 * build bundles it into this file, because bundlers treat a worklet URL as a
 * plain asset and would not follow its imports.
 */
class AlgoWasmProcessor extends AudioWorkletProcessor {
  private engine?: EngineCore;
  private playing = false;
  private destroyed = false;
  private framesSincePoll = 0;
  private commandQueue: Promise<void> = Promise.resolve();

  constructor() {
    super();
    this.port.onmessage = event => {
      const command = event.data as WorkletCommand;

      // Chain onto the queue instead of calling handleCommand directly, so
      // commands are always applied in the order they were received, even
      // while an earlier asynchronous command (such as 'configure') is still
      // pending.
      this.commandQueue = this.commandQueue.then(() => this.handleCommand(command));
    };
  }

  override process(
    _inputs: Float32Array[][],
    outputs: Float32Array[][],
    _parameters: Record<string, Float32Array>
  ): boolean {
    // Returning false lets the browser release a destroyed processor.
    // Otherwise it would keep running for as long as a caller-owned
    // AudioContext stays open.
    if (this.destroyed) {
      return false;
    }

    const output = outputs[0];
    const left = output?.[0];
    const right = output?.[1] ?? left;

    if (!left || !right) {
      return true;
    }

    if (!this.engine || !this.playing) {
      left.fill(0);
      right.fill(0);
      return true;
    }

    try {
      this.engine.renderPlanar(left, right);
      this.framesSincePoll += left.length;

      if (this.framesSincePoll >= EVENT_POLL_FRAMES) {
        this.framesSincePoll = 0;
        this.postEvents();
      }
    } catch (error) {
      this.playing = false;
      left.fill(0);
      right.fill(0);
      this.port.postMessage({ type: 'error', error: messageOf(error) });
    }

    return true;
  }

  /**
   * Posts the events buffered since the last poll, with a fresh snapshot.
   * Both travel as transferred binary buffers, which the player decodes, so
   * the audio thread builds no objects or strings for them.
   */
  private postEvents(): void {
    const events = this.engine?.copyEvents();

    if (!events || !this.engine) {
      return;
    }

    const snapshot = this.engine.copySnapshot();
    this.port.postMessage({ type: 'events', events, snapshot }, [events.buffer, snapshot.buffer]);
  }

  private async handleCommand(command: WorkletCommand): Promise<void> {
    try {
      if (command.type === 'configure') {
        await this.configure(command);
        return;
      }

      const engine = this.engine;

      if (!engine) {
        // A player whose set-up failed still sends 'destroy'. Honour it even
        // though no engine exists, so the processor can be released.
        if (command.type === 'destroy') {
          this.destroyed = true;
          this.reply(command.id, true);
          return;
        }

        throw new Error('AlgoWASM worklet has not been configured.');
      }

      const result = this.applyCommand(engine, command);
      // A destroyed engine has no snapshot to report.
      const snapshot = this.engine ? engine.copySnapshot() : undefined;
      this.reply(command.id, true, snapshot, result);
    } catch (error) {
      this.reply(command.id, false, undefined, undefined, messageOf(error));
    }
  }

  private async configure(command: WorkletCommand): Promise<void> {
    if (!command.wasmBytes || !command.profile) {
      throw new Error('WASM bytes and a profile are required to configure the worklet.');
    }

    const { instance } = await WebAssembly.instantiate(command.wasmBytes, {});
    const engine = new EngineCore(instance);
    engine.createFromWire(
      command.sampleRate ?? sampleRate,
      command.seedHigh ?? 0,
      command.seedLow ?? 1,
      command.profile
    );
    engine.setVolume(command.value ?? 0.8);
    this.engine = engine;
    const snapshot = engine.copySnapshot();
    this.port.postMessage({ type: 'ready', snapshot }, [snapshot.buffer]);
  }

  private applyCommand(engine: EngineCore, command: WorkletCommand): ArrayBuffer | undefined {
    switch (command.type) {
      case 'start':
        engine.start();
        this.playing = true;
        return undefined;
      case 'pause':
        engine.pause();
        this.playing = false;
        return undefined;
      case 'resume':
        engine.resume();
        this.playing = true;
        return undefined;
      case 'stop':
        engine.stop();
        this.playing = false;
        return undefined;
      case 'set-seed':
        engine.setSeedParts(command.seedHigh ?? 0, command.seedLow ?? 1);
        // Re-seeding stops the engine, so playback must be started again.
        this.playing = false;
        return undefined;
      case 'set-profile':
        if (!command.profile) {
          throw new Error('A profile is required.');
        }

        engine.setProfileWire(command.profile);
        this.playing = false;
        return undefined;
      case 'set-volume':
        engine.setVolume(command.value ?? 0.8);
        return undefined;
      case 'set-energy':
        engine.setEnergy(command.value ?? 0.5);
        return undefined;
      case 'set-intensity':
        engine.setIntensity(command.value ?? 0.5);
        return undefined;
      case 'set-mood':
        engine.setMoodIndex(command.mood ?? 0);
        return undefined;
      case 'set-tempo-range':
        engine.setTempoRange(command.minBpm ?? 0, command.maxBpm ?? 0);
        return undefined;
      case 'set-muted':
        engine.setMutedIndex(command.track ?? -1, command.flag ?? false);
        return undefined;
      case 'set-solo':
        engine.setSoloIndex(command.track ?? -1, command.flag ?? false);
        return undefined;
      case 'export-midi':
        return engine.exportMidi().buffer as ArrayBuffer;
      case 'destroy':
        engine.destroy();
        this.engine = undefined;
        this.playing = false;
        this.destroyed = true;
        return undefined;
      default:
        throw new Error(`Unknown AlgoWASM command: ${command.type}.`);
    }
  }

  private reply(
    id: number | undefined,
    ok: boolean,
    snapshot?: Float64Array,
    result?: ArrayBuffer,
    error?: string
  ): void {
    if (id === undefined) {
      // Commands without an id come from configure, which reports failure
      // through the error channel instead of a reply.
      if (!ok && error) {
        this.port.postMessage({ type: 'error', error });
      }

      return;
    }

    this.port.postMessage({ type: 'command-result', id, ok, snapshot, result, error });
  }
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

registerProcessor('algo-wasm-processor', AlgoWasmProcessor);
