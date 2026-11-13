import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { existsSync } from 'node:fs';

// These tests exercise the built package, so it must exist first. Checking
// here gives a clear instruction instead of a module resolution stack trace.
if (!existsSync(new URL('../../packages/algo-wasm/dist/index.js', import.meta.url))) {
  throw new Error('The package has not been built. Run "npm install" and "npm run build" first.');
}

const { AlgoWasmEngineBinding, AlgoWasmError, builtInProfiles, normaliseSeed, validateProfile } =
  await import('../../packages/algo-wasm/dist/index.js');

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

    // Long enough to pass the limiter's few milliseconds of look-ahead.
    const output = new Float32Array(4096);
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

test('every built-in profile validates and renders finite non-silent audio', async () => {
  const wasmBytes = await readFile(
    new URL('../../packages/algo-wasm/dist/algo_wasm.wasm', import.meta.url)
  );

  for (const createProfile of Object.values(builtInProfiles)) {
    const { instance } = await WebAssembly.instantiate(wasmBytes, {});
    const binding = new AlgoWasmEngineBinding(instance);

    try {
      binding.create(48000, 7n, validateProfile(createProfile()));
      binding.start();

      // Long enough to pass the limiter's few milliseconds of look-ahead.
      const output = new Float32Array(4096);
      binding.renderTo(output);

      assert.equal(output.every(Number.isFinite), true);
      assert.equal(output.some(sample => Math.abs(sample) > 0.0001), true);
    } finally {
      binding.destroy();
    }
  }
});

test('hostile profile values and control inputs are rejected with clear errors', async () => {
  const hostile = builtInProfiles.dubstep();
  hostile.limits.maxEventsPerBlock = 4_000_000_000;
  assert.throws(() => validateProfile(hostile), /maxEventsPerBlock must be a whole number from 1 to 512/);

  const unknownScale = builtInProfiles.dubstep();
  unknownScale.scales[0].name = 'lydian';
  assert.throws(() => validateProfile(unknownScale), /Scale name must be one of/);

  const wasmBytes = await readFile(
    new URL('../../packages/algo-wasm/dist/algo_wasm.wasm', import.meta.url)
  );
  const { instance } = await WebAssembly.instantiate(wasmBytes, {});
  const binding = new AlgoWasmEngineBinding(instance);

  try {
    binding.create(48000, 1n, builtInProfiles.amigaHouse95ish());
    assert.throws(() => binding.setTempoRange(Number.NaN, Number.NaN), /Tempo range must be finite/);
    assert.throws(() => binding.setMood('cheerful'), TypeError);
    assert.throws(() => binding.setMuted('cowbell', true), TypeError);
  } finally {
    binding.destroy();
  }
});

test('no beat is lost when events are read less often than audio is rendered', async () => {
  const wasmBytes = await readFile(
    new URL('../../packages/algo-wasm/dist/algo_wasm.wasm', import.meta.url)
  );
  const { instance } = await WebAssembly.instantiate(wasmBytes, {});
  const binding = new AlgoWasmEngineBinding(instance);

  try {
    binding.create(48000, 777n, builtInProfiles.amigaHouse95ish());
    binding.start();

    const block = new Float32Array(256);
    let beats = 0;

    // Ten seconds of 128-frame blocks, with events read every eighth block.
    for (let index = 0; index < 3750; index += 1) {
      binding.renderTo(block);

      if (index % 8 === 7) {
        beats += binding.events().filter(event => event.type === 'beat').length;
      }
    }

    const expected = Math.floor((binding.snapshot().bpm / 60) * 10);
    assert.ok(beats >= expected - 1, `saw ${beats} beats, expected about ${expected}`);
  } finally {
    binding.destroy();
  }
});

test('release artefacts stay inside their size budgets', async () => {
  // Budgets sit a little above the 1.2.0 sizes, to catch accidental growth.
  const budgets = {
    'algo_wasm.wasm': 120_000,
    'index.js': 40_000,
    'worklet/algo-worklet.js': 10_000
  };

  for (const [file, budget] of Object.entries(budgets)) {
    const bytes = await readFile(new URL(`../../packages/algo-wasm/dist/${file}`, import.meta.url));
    assert.ok(bytes.length <= budget, `${file} is ${bytes.length} bytes, over its ${budget} byte budget`);
  }
});
