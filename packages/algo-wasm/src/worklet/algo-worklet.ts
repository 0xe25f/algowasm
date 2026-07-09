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

type WasmFn = (...args: number[]) => number;

type WorkletCommand = {
  type: string;
  id?: number;
  wasmUrl?: string;
  wasmBytes?: ArrayBuffer;
  profile?: unknown;
  seed?: string;
  sampleRate?: number;
  volume?: number;
  value?: number;
  trackId?: string;
  muted?: boolean;
  solo?: boolean;
  mood?: string;
  minBpm?: number;
  maxBpm?: number;
};

type WorkletExports = Record<string, WebAssembly.ExportValue>;

class WorkletWasmBinding {
  private readonly exports: WorkletExports;
  private enginePtr = 0;
  private renderPtr = 0;
  private renderLength = 0;
  private renderView?: Float32Array;

  constructor(instance: WebAssembly.Instance) {
    this.exports = instance.exports as WorkletExports;
    this.memory;
  }

  create(profile: unknown, seed: string, requestedSampleRate: number, volume: number): void {
    if (this.enginePtr !== 0) {
      this.destroy();
    }

    const profileJson = JSON.stringify(profile);
    const { high, low } = splitSeed(seed);
    const pointer = this.writeString(profileJson);

    try {
      const result = this.call(
        'algo_wasm_engine_create',
        requestedSampleRate,
        high,
        low,
        pointer.pointer,
        pointer.length
      );

      if (result === 0) {
        throw new Error(this.lastError('Engine creation failed.'));
      }

      this.enginePtr = result;
      this.setVolume(volume);
    } finally {
      this.call('algo_wasm_dealloc', pointer.pointer, pointer.length);
    }
  }

  start(): void {
    this.callCode('algo_wasm_engine_start');
  }

  pause(): void {
    this.callCode('algo_wasm_engine_pause');
  }

  resume(): void {
    this.callCode('algo_wasm_engine_resume');
  }

  stop(): void {
    this.callCode('algo_wasm_engine_stop');
  }

  setSeed(seed: string): void {
    const { high, low } = splitSeed(seed);
    this.callCode('algo_wasm_engine_set_seed', high, low);
  }

  setProfile(profile: unknown): void {
    const pointer = this.writeString(JSON.stringify(profile));

    try {
      const code = this.call('algo_wasm_engine_set_profile', this.enginePtr, pointer.pointer, pointer.length);
      this.assertCode(code, 'Profile update failed.');
    } finally {
      this.call('algo_wasm_dealloc', pointer.pointer, pointer.length);
    }
  }

  setVolume(value: number): void {
    this.callCode('algo_wasm_engine_set_volume', clampUnit(value));
  }

  setEnergy(value: number): void {
    this.callCode('algo_wasm_engine_set_energy', clampUnit(value));
  }

  setIntensity(value: number): void {
    this.callCode('algo_wasm_engine_set_intensity', clampUnit(value));
  }

  setMood(mood: string): void {
    const pointer = this.writeString(mood);

    try {
      const code = this.call('algo_wasm_engine_set_mood', this.enginePtr, pointer.pointer, pointer.length);
      this.assertCode(code, 'Mood update failed.');
    } finally {
      this.call('algo_wasm_dealloc', pointer.pointer, pointer.length);
    }
  }

  setTempoRange(minBpm: number, maxBpm: number): void {
    this.callCode('algo_wasm_engine_set_tempo_range', minBpm, maxBpm);
  }

  setMuted(trackId: string, muted: boolean): void {
    const pointer = this.writeString(trackId);

    try {
      const code = this.call(
        'algo_wasm_engine_set_muted',
        this.enginePtr,
        pointer.pointer,
        pointer.length,
        muted ? 1 : 0
      );
      this.assertCode(code, 'Mute update failed.');
    } finally {
      this.call('algo_wasm_dealloc', pointer.pointer, pointer.length);
    }
  }

  setSolo(trackId: string, solo: boolean): void {
    const pointer = this.writeString(trackId);

    try {
      const code = this.call(
        'algo_wasm_engine_set_solo',
        this.enginePtr,
        pointer.pointer,
        pointer.length,
        solo ? 1 : 0
      );
      this.assertCode(code, 'Solo update failed.');
    } finally {
      this.call('algo_wasm_dealloc', pointer.pointer, pointer.length);
    }
  }

  renderTo(output: Float32Array): void {
    this.ensureRenderBuffer(output.length);
    const code = this.call('algo_wasm_engine_render', this.enginePtr, this.renderPtr, output.length);
    this.assertCode(code, 'Render failed.');

    if (!this.renderView || this.renderView.buffer !== this.memory.buffer) {
      this.renderView = new Float32Array(this.memory.buffer, this.renderPtr, this.renderLength);
    }

    output.set(this.renderView.subarray(0, output.length));
  }

  snapshot(): unknown {
    const length = this.call('algo_wasm_engine_snapshot_json', this.enginePtr);
    return this.readJson(length);
  }

  events(): unknown[] {
    const length = this.call('algo_wasm_engine_events_json', this.enginePtr);
    return this.readJson(length) as unknown[];
  }

  exportMidi(): Uint8Array {
    const length = this.call('algo_wasm_engine_export_midi', this.enginePtr);
    const pointer = this.call('algo_wasm_bytes_ptr');
    return new Uint8Array(this.memory.buffer, pointer, length).slice();
  }

  destroy(): void {
    if (this.renderPtr !== 0) {
      this.call('algo_wasm_dealloc', this.renderPtr, this.renderLength * 4);
      this.renderPtr = 0;
    }

    if (this.enginePtr !== 0) {
      this.call('algo_wasm_engine_destroy', this.enginePtr);
      this.enginePtr = 0;
    }
  }

  private get memory(): WebAssembly.Memory {
    const memory = this.exports.memory;

    if (!(memory instanceof WebAssembly.Memory)) {
      throw new Error('AlgoWASM did not export WebAssembly memory.');
    }

    return memory;
  }

  private ensureRenderBuffer(length: number): void {
    if (this.renderPtr !== 0 && this.renderLength >= length) {
      return;
    }

    if (this.renderPtr !== 0) {
      this.call('algo_wasm_dealloc', this.renderPtr, this.renderLength * 4);
    }

    this.renderLength = length;
    this.renderPtr = this.call('algo_wasm_alloc', length * 4);
    this.renderView = new Float32Array(this.memory.buffer, this.renderPtr, length);
  }

  private writeString(value: string): { pointer: number; length: number } {
    const bytes = encodeUtf8(value);
    const pointer = this.call('algo_wasm_alloc', bytes.length);

    if (pointer === 0) {
      throw new Error('Could not allocate a WASM string buffer.');
    }

    new Uint8Array(this.memory.buffer, pointer, bytes.length).set(bytes);
    return {
      pointer,
      length: bytes.length
    };
  }

  private readJson(length: number): unknown {
    if (length === 0) {
      throw new Error(this.lastError('AlgoWASM returned an empty JSON buffer.'));
    }

    const pointer = this.call('algo_wasm_json_ptr');
    const bytes = new Uint8Array(this.memory.buffer, pointer, length);
    return JSON.parse(decodeUtf8(bytes));
  }

  private callCode(name: string, ...args: number[]): void {
    const code = this.call(name, this.enginePtr, ...args);
    this.assertCode(code, `${String(name)} failed.`);
  }

  private assertCode(code: number, fallback: string): void {
    if (code !== 0) {
      throw new Error(this.lastError(fallback));
    }
  }

  private call(name: string, ...args: number[]): number {
    const value = this.exports[name];

    if (typeof value !== 'function') {
      throw new Error(`AlgoWASM is missing the ${String(name)} export.`);
    }

    return (value as WasmFn)(...args);
  }

  private lastError(fallback: string): string {
    const length = this.call('algo_wasm_last_error_len');

    if (length === 0) {
      return fallback;
    }

    const pointer = this.call('algo_wasm_last_error_ptr');
    const bytes = new Uint8Array(this.memory.buffer, pointer, length);
    return decodeUtf8(bytes);
  }
}

class AlgoWasmProcessor extends AudioWorkletProcessor {
  private binding?: WorkletWasmBinding;
  private playing = false;
  private scratch = new Float32Array(0);
  private eventCountdown = 0;
  private commandQueue: Promise<void> = Promise.resolve();

  constructor() {
    super();
    this.port.onmessage = event => {
      const command = event.data as WorkletCommand;

      // Chain onto the queue instead of firing handleCommand directly so
      // commands are always applied in the order they were received, even
      // while an earlier async command (such as 'configure') is still
      // pending.
      this.commandQueue = this.commandQueue.then(() => this.handleCommand(command));
    };
  }

  override process(
    _inputs: Float32Array[][],
    outputs: Float32Array[][],
    _parameters: Record<string, Float32Array>
  ): boolean {
    const output = outputs[0];
    const left = output?.[0];
    const right = output?.[1] ?? left;

    if (!left || !right) {
      return true;
    }

    if (!this.binding || !this.playing) {
      left.fill(0);
      right.fill(0);
      return true;
    }

    const length = left.length * 2;

    if (this.scratch.length !== length) {
      this.scratch = new Float32Array(length);
    }

    try {
      this.binding.renderTo(this.scratch);

      for (let index = 0; index < left.length; index += 1) {
        left[index] = this.scratch[index * 2] ?? 0;
        right[index] = this.scratch[index * 2 + 1] ?? 0;
      }

      this.eventCountdown -= 1;

      if (this.eventCountdown <= 0) {
        this.eventCountdown = 16;
        const events = this.binding.events();

        if (events.length > 0) {
          this.port.postMessage({
            type: 'events',
            events,
            snapshot: this.binding.snapshot()
          });
        }
      }
    } catch (error) {
      this.playing = false;
      left.fill(0);
      right.fill(0);
      this.port.postMessage({
        type: 'error',
        error: error instanceof Error ? error.message : String(error)
      });
    }

    return true;
  }

  private async handleCommand(command: WorkletCommand): Promise<void> {
    try {
      if (command.type === 'configure') {
        await this.configure(command);
        return;
      }

      if (!this.binding) {
        throw new Error('AlgoWASM worklet has not been configured.');
      }

      const result = this.applyCommand(command);
      this.reply(command.id, true, result);
    } catch (error) {
      this.reply(command.id, false, undefined, error instanceof Error ? error.message : String(error));
    }
  }

  private async configure(command: WorkletCommand): Promise<void> {
    if (!command.wasmBytes && !command.wasmUrl) {
      throw new Error('WASM bytes or a WASM URL are required.');
    }

    const instance = await instantiateWorkletWasm(command.wasmUrl, command.wasmBytes);
    const binding = new WorkletWasmBinding(instance);
    binding.create(command.profile ?? {}, command.seed ?? '1', command.sampleRate ?? sampleRate, command.volume ?? 0.8);
    this.binding = binding;
    this.port.postMessage({
      type: 'ready',
      snapshot: binding.snapshot()
    });
  }

  private applyCommand(command: WorkletCommand): unknown {
    const binding = this.requireBinding();

    switch (command.type) {
      case 'start':
        binding.start();
        this.playing = true;
        return binding.snapshot();
      case 'pause':
        binding.pause();
        this.playing = false;
        return binding.snapshot();
      case 'resume':
        binding.resume();
        this.playing = true;
        return binding.snapshot();
      case 'stop':
        binding.stop();
        this.playing = false;
        return binding.snapshot();
      case 'set-seed':
        binding.setSeed(command.seed ?? '1');
        return binding.snapshot();
      case 'set-profile':
        binding.setProfile(command.profile ?? {});
        return binding.snapshot();
      case 'set-volume':
        binding.setVolume(command.value ?? 0.8);
        return binding.snapshot();
      case 'set-energy':
        binding.setEnergy(command.value ?? 0.5);
        return binding.snapshot();
      case 'set-intensity':
        binding.setIntensity(command.value ?? 0.5);
        return binding.snapshot();
      case 'set-mood':
        binding.setMood(command.mood ?? 'dark');
        return binding.snapshot();
      case 'set-tempo-range':
        binding.setTempoRange(command.minBpm ?? 118, command.maxBpm ?? 138);
        return binding.snapshot();
      case 'set-muted':
        binding.setMuted(command.trackId ?? '', command.muted ?? false);
        return binding.snapshot();
      case 'set-solo':
        binding.setSolo(command.trackId ?? '', command.solo ?? false);
        return binding.snapshot();
      case 'export-midi': {
        const bytes = binding.exportMidi();
        return bytes.buffer;
      }
      case 'destroy':
        binding.destroy();
        this.binding = undefined;
        this.playing = false;
        return undefined;
      default:
        throw new Error(`Unknown AlgoWASM command: ${command.type}.`);
    }
  }

  private requireBinding(): WorkletWasmBinding {
    if (!this.binding) {
      throw new Error('AlgoWASM worklet has not been configured.');
    }

    return this.binding;
  }

  private reply(id: number | undefined, ok: boolean, result?: unknown, error?: string): void {
    if (id === undefined) {
      if (!ok && error) {
        this.port.postMessage({
          type: 'error',
          error
        });
      }

      return;
    }

    this.port.postMessage({
      type: 'command-result',
      id,
      ok,
      result,
      error
    });
  }
}

async function instantiateWorkletWasm(
  url: string | undefined,
  wasmBytes: ArrayBuffer | undefined
): Promise<WebAssembly.Instance> {
  if (wasmBytes) {
    const result = await WebAssembly.instantiate(wasmBytes, {});
    return result.instance;
  }

  if (!url) {
    throw new Error('WASM URL is required when bytes are not supplied.');
  }

  const response = await fetch(url);

  if (!response.ok) {
    throw new Error(`Could not load AlgoWASM from ${url}. The server returned ${response.status}.`);
  }

  if (WebAssembly.instantiateStreaming) {
    try {
      const result = await WebAssembly.instantiateStreaming(response, {});
      return result.instance;
    } catch (error) {
      if (response.headers.get('content-type') === 'application/wasm') {
        throw error;
      }
    }
  }

  const fallback = await fetch(url);
  const bytes = await fallback.arrayBuffer();
  const result = await WebAssembly.instantiate(bytes, {});
  return result.instance;
}

function splitSeed(seed: string): { high: number; low: number } {
  const value = BigInt.asUintN(64, BigInt(seed));

  return {
    high: Number((value >> 32n) & 0xffff_ffffn),
    low: Number(value & 0xffff_ffffn)
  };
}

function clampUnit(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }

  return Math.min(1, Math.max(0, value));
}

function encodeUtf8(value: string): Uint8Array {
  if (typeof TextEncoder !== 'undefined') {
    return new TextEncoder().encode(value);
  }

  const bytes: number[] = [];

  for (const character of value) {
    const point = character.codePointAt(0) ?? 0;

    if (point <= 0x7f) {
      bytes.push(point);
    } else if (point <= 0x7ff) {
      bytes.push(0xc0 | (point >> 6), 0x80 | (point & 0x3f));
    } else if (point <= 0xffff) {
      bytes.push(0xe0 | (point >> 12), 0x80 | ((point >> 6) & 0x3f), 0x80 | (point & 0x3f));
    } else {
      bytes.push(
        0xf0 | (point >> 18),
        0x80 | ((point >> 12) & 0x3f),
        0x80 | ((point >> 6) & 0x3f),
        0x80 | (point & 0x3f)
      );
    }
  }

  return new Uint8Array(bytes);
}

function decodeUtf8(bytes: Uint8Array): string {
  if (typeof TextDecoder !== 'undefined') {
    return new TextDecoder().decode(bytes);
  }

  let output = '';
  let index = 0;

  while (index < bytes.length) {
    const first = bytes[index] ?? 0;

    if (first < 0x80) {
      output += String.fromCodePoint(first);
      index += 1;
    } else if (first < 0xe0) {
      const second = bytes[index + 1] ?? 0;
      output += String.fromCodePoint(((first & 0x1f) << 6) | (second & 0x3f));
      index += 2;
    } else if (first < 0xf0) {
      const second = bytes[index + 1] ?? 0;
      const third = bytes[index + 2] ?? 0;
      output += String.fromCodePoint(
        ((first & 0x0f) << 12) | ((second & 0x3f) << 6) | (third & 0x3f)
      );
      index += 3;
    } else {
      const second = bytes[index + 1] ?? 0;
      const third = bytes[index + 2] ?? 0;
      const fourth = bytes[index + 3] ?? 0;
      output += String.fromCodePoint(
        ((first & 0x07) << 18)
          | ((second & 0x3f) << 12)
          | ((third & 0x3f) << 6)
          | (fourth & 0x3f)
      );
      index += 4;
    }
  }

  return output;
}

registerProcessor('algo-wasm-processor', AlgoWasmProcessor);
