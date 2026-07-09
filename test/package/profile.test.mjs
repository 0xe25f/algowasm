import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import {
  AlgoWasmEngineBinding,
  AlgoWasmError,
  builtInProfiles,
  normaliseSeed,
  validateProfile
} from '../../packages/algo-wasm/dist/index.js';

test('built-in profile validates and is cloned', () => {
  const profile = builtInProfiles.amigaHouse95ish();
  const validated = validateProfile(profile);

  assert.equal(validated.id, 'amiga_house_95ish');
  validated.tempo.defaultBpm = 120;
  assert.equal(profile.tempo.defaultBpm, 126);
});

test('downtempo breakbeat profile validates', () => {
  const profile = builtInProfiles.downtempoBreakbeat();
  const validated = validateProfile(profile);

  assert.equal(validated.id, 'downtempo_breakbeat');
  assert.ok(validated.tempo.maxBpm <= 112);
});

test('dub deep house profile validates', () => {
  const profile = builtInProfiles.dubDeepHouse();
  const validated = validateProfile(profile);

  assert.equal(validated.id, 'dub_deep_house');
  assert.ok(validated.tempo.minBpm >= 118);
});

test('invalid profiles fail with a clear error', () => {
  const profile = builtInProfiles.amigaHouse95ish();
  profile.tempo.minBpm = 160;
  profile.tempo.maxBpm = 120;

  assert.throws(() => validateProfile(profile), AlgoWasmError);
});

test('seed normalisation accepts large decimal strings', () => {
  assert.equal(normaliseSeed('18446744073709551616'), 0n);
  assert.equal(normaliseSeed(42), 42n);
});

test('compiled WASM renders finite non-silent audio and exports MIDI', async () => {
  const wasmBytes = await readFile(
    new URL('../../packages/algo-wasm/dist/algo_wasm.wasm', import.meta.url)
  );
  const { instance } = await WebAssembly.instantiate(wasmBytes, {});
  const binding = new AlgoWasmEngineBinding(instance);

  try {
    binding.create(48000, 42n, builtInProfiles.amigaHouse95ish());
    binding.start();

    const output = new Float32Array(512);
    binding.renderTo(output);

    assert.equal(output.every(Number.isFinite), true);
    assert.equal(output.some(sample => Math.abs(sample) > 0.0001), true);

    const snapshot = binding.snapshot();
    assert.equal(snapshot.seed, '42');
    assert.equal(snapshot.sampleRate, 48000);

    const events = binding.events();
    assert.equal(events.some(event => event.type === 'bar'), true);

    const midi = binding.exportMidi();
    assert.equal(new TextDecoder().decode(midi.subarray(0, 4)), 'MThd');
  } finally {
    binding.destroy();
  }
});

test('additional built-in profiles render finite non-silent audio', async () => {
  const wasmBytes = await readFile(
    new URL('../../packages/algo-wasm/dist/algo_wasm.wasm', import.meta.url)
  );

  for (const profile of [builtInProfiles.downtempoBreakbeat(), builtInProfiles.dubDeepHouse()]) {
    const { instance } = await WebAssembly.instantiate(wasmBytes, {});
    const binding = new AlgoWasmEngineBinding(instance);

    try {
      binding.create(48000, 7n, profile);
      binding.start();

      const output = new Float32Array(512);
      binding.renderTo(output);

      assert.equal(output.every(Number.isFinite), true);
      assert.equal(output.some(sample => Math.abs(sample) > 0.0001), true);
    } finally {
      binding.destroy();
    }
  }
});
