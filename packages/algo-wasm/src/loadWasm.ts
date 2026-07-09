import { AlgoWasmError } from './AlgoWasmError.js';
import { splitSeed } from './profile.js';
import type { AlgoWasmGeneratedEvent, AlgoWasmProfile, AlgoWasmSnapshot } from './types.js';

export interface AlgoWasmInstantiation {
  instance: WebAssembly.Instance;
  module: WebAssembly.Module;
}

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

type WasmFn = (...args: number[]) => number;
type AlgoWasmExports = Record<string, WebAssembly.ExportValue>;

export class AlgoWasmEngineBinding {
  private readonly exports: AlgoWasmExports;
  private readonly encoder = new TextEncoder();
  private readonly decoder = new TextDecoder();
  private enginePtr = 0;
  private renderPtr = 0;
  private renderLength = 0;
  private renderView?: Float32Array;

  constructor(instance: WebAssembly.Instance) {
    this.exports = instance.exports as AlgoWasmExports;
    this.requireMemory();
    this.requireExport('algo_wasm_alloc');
    this.requireExport('algo_wasm_dealloc');
  }

  create(sampleRate: number, seed: bigint, profile: AlgoWasmProfile): void {
    if (this.enginePtr !== 0) {
      this.destroy();
    }

    const { high, low } = splitSeed(seed);
    const profileJson = JSON.stringify(profile);
    const result = this.withString(profileJson, (pointer, length) =>
      this.call('algo_wasm_engine_create', sampleRate, high, low, pointer, length)
    );

    if (result === 0) {
      throw new AlgoWasmError('engine_failed', this.readLastError('AlgoWASM engine creation failed.'));
    }

    this.enginePtr = result;
  }

  start(): void {
    this.callEngine('algo_wasm_engine_start');
  }

  pause(): void {
    this.callEngine('algo_wasm_engine_pause');
  }

  resume(): void {
    this.callEngine('algo_wasm_engine_resume');
  }

  stop(): void {
    this.callEngine('algo_wasm_engine_stop');
  }

  setSeed(seed: bigint): void {
    const { high, low } = splitSeed(seed);
    this.callEngine('algo_wasm_engine_set_seed', high, low);
  }

  setProfile(profile: AlgoWasmProfile): void {
    const profileJson = JSON.stringify(profile);
    const result = this.withString(profileJson, (pointer, length) =>
      this.call('algo_wasm_engine_set_profile', this.enginePtr, pointer, length)
    );
    this.assertCode(result, 'Profile update failed.');
  }

  setVolume(value: number): void {
    this.callEngine('algo_wasm_engine_set_volume', value);
  }

  setEnergy(value: number): void {
    this.callEngine('algo_wasm_engine_set_energy', value);
  }

  setIntensity(value: number): void {
    this.callEngine('algo_wasm_engine_set_intensity', value);
  }

  setMood(value: string): void {
    const result = this.withString(value, (pointer, length) =>
      this.call('algo_wasm_engine_set_mood', this.enginePtr, pointer, length)
    );
    this.assertCode(result, 'Mood update failed.');
  }

  setTempoRange(minBpm: number, maxBpm: number): void {
    this.callEngine('algo_wasm_engine_set_tempo_range', minBpm, maxBpm);
  }

  setMuted(trackId: string, muted: boolean): void {
    const result = this.withString(trackId, (pointer, length) =>
      this.call('algo_wasm_engine_set_muted', this.enginePtr, pointer, length, muted ? 1 : 0)
    );
    this.assertCode(result, 'Mute update failed.');
  }

  setSolo(trackId: string, solo: boolean): void {
    const result = this.withString(trackId, (pointer, length) =>
      this.call('algo_wasm_engine_set_solo', this.enginePtr, pointer, length, solo ? 1 : 0)
    );
    this.assertCode(result, 'Solo update failed.');
  }

  renderTo(output: Float32Array): void {
    this.ensureEngine();
    this.ensureRenderBuffer(output.length);
    const result = this.call('algo_wasm_engine_render', this.enginePtr, this.renderPtr, output.length);
    this.assertCode(result, 'Render failed.');

    if (!this.renderView || this.renderView.length !== output.length) {
      this.renderView = new Float32Array(this.memory.buffer, this.renderPtr, output.length);
    }

    output.set(this.renderView);
  }

  snapshot(): AlgoWasmSnapshot {
    this.ensureEngine();
    const length = this.call('algo_wasm_engine_snapshot_json', this.enginePtr);
    return this.readJson(length) as AlgoWasmSnapshot;
  }

  events(): AlgoWasmGeneratedEvent[] {
    this.ensureEngine();
    const length = this.call('algo_wasm_engine_events_json', this.enginePtr);
    return this.readJson(length) as AlgoWasmGeneratedEvent[];
  }

  exportMidi(): Uint8Array {
    this.ensureEngine();
    const length = this.call('algo_wasm_engine_export_midi', this.enginePtr);
    const pointer = this.call('algo_wasm_bytes_ptr');

    return new Uint8Array(this.memory.buffer, pointer, length).slice();
  }

  destroy(): void {
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

  private ensureRenderBuffer(length: number): void {
    if (this.renderPtr !== 0 && this.renderLength >= length) {
      if (this.renderView?.buffer !== this.memory.buffer) {
        this.renderView = new Float32Array(this.memory.buffer, this.renderPtr, this.renderLength);
      }

      return;
    }

    if (this.renderPtr !== 0) {
      this.call('algo_wasm_dealloc', this.renderPtr, this.renderLength * 4);
    }

    this.renderLength = length;
    this.renderPtr = this.call('algo_wasm_alloc', length * 4);

    if (this.renderPtr === 0) {
      throw new AlgoWasmError('render_failed', 'Could not allocate the WASM render buffer.');
    }

    this.renderView = new Float32Array(this.memory.buffer, this.renderPtr, length);
  }

  private get memory(): WebAssembly.Memory {
    return this.requireMemory();
  }

  private requireMemory(): WebAssembly.Memory {
    const memory = this.exports.memory;

    if (!(memory instanceof WebAssembly.Memory)) {
      throw new AlgoWasmError('wasm_load_failed', 'AlgoWASM did not export a WebAssembly memory.');
    }

    return memory;
  }

  private requireExport(name: string): WasmFn {
    const fn = this.exports[name];

    if (typeof fn !== 'function') {
      throw new AlgoWasmError('wasm_load_failed', `AlgoWASM is missing the ${name} export.`);
    }

    return fn as WasmFn;
  }

  private call(name: string, ...args: number[]): number {
    return this.requireExport(name)(...args);
  }

  private callEngine(name: string, ...args: number[]): void {
    this.ensureEngine();
    const result = this.call(name, this.enginePtr, ...args);
    this.assertCode(result, `${String(name)} failed.`);
  }

  private assertCode(result: number, fallback: string): void {
    if (result !== 0) {
      throw new AlgoWasmError('engine_failed', this.readLastError(fallback));
    }
  }

  private ensureEngine(): void {
    if (this.enginePtr === 0) {
      throw new AlgoWasmError('engine_failed', 'AlgoWASM engine has not been created.');
    }
  }

  private withString<T>(value: string, callback: (pointer: number, length: number) => T): T {
    const bytes = this.encoder.encode(value);
    const pointer = this.call('algo_wasm_alloc', bytes.length);

    if (pointer === 0) {
      throw new AlgoWasmError('engine_failed', 'Could not allocate a WASM string buffer.');
    }

    new Uint8Array(this.memory.buffer, pointer, bytes.length).set(bytes);

    try {
      return callback(pointer, bytes.length);
    } finally {
      this.call('algo_wasm_dealloc', pointer, bytes.length);
    }
  }

  private readJson(length: number): unknown {
    if (length === 0) {
      const error = this.readLastError('AlgoWASM returned an empty JSON buffer.');
      throw new AlgoWasmError('engine_failed', error);
    }

    const pointer = this.call('algo_wasm_json_ptr');
    const bytes = new Uint8Array(this.memory.buffer, pointer, length);
    return JSON.parse(this.decoder.decode(bytes));
  }

  private readLastError(fallback: string): string {
    const length = this.call('algo_wasm_last_error_len');

    if (length === 0) {
      return fallback;
    }

    const pointer = this.call('algo_wasm_last_error_ptr');
    const bytes = new Uint8Array(this.memory.buffer, pointer, length);
    return this.decoder.decode(bytes);
  }
}
