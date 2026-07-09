import { AlgoWasmPlayer, builtInProfiles } from './vendor/algo-wasm/index.js';

const startButton = document.querySelector('#start');
const pauseButton = document.querySelector('#pause');
const stopButton = document.querySelector('#stop');
const randomButton = document.querySelector('#random');
const seedInput = document.querySelector('#seedInput');
const lengthInput = document.querySelector('#lengthInput');
const profileInput = document.querySelector('#profileInput');
const energyInput = document.querySelector('#energy');
const intensityInput = document.querySelector('#intensity');
const moodInput = document.querySelector('#mood');
const statusOutput = document.querySelector('#status');
const timeOutput = document.querySelector('#time');
const sectionOutput = document.querySelector('#section');
const barOutput = document.querySelector('#bar');
const seedOutput = document.querySelector('#seed');

// The dropdown label always matches the profile's own displayName, so the
// demo cannot drift out of sync with the profiles the package ships.
const profileFactories = {
  amiga_house_95ish: builtInProfiles.amigaHouse95ish,
  downtempo_breakbeat: builtInProfiles.downtempoBreakbeat,
  dub_deep_house: builtInProfiles.dubDeepHouse
};

for (const [id, factory] of Object.entries(profileFactories)) {
  const option = document.createElement('option');
  option.value = id;
  option.textContent = factory().displayName;
  profileInput?.append(option);
}

let player;
let playerPromise;
let activeSeed = 1995n;
let engineSeed;
let activeProfileId = profileInput?.value ?? 'amiga_house_95ish';
let trackLengthSeconds = 120;
let elapsedBeforeStartMs = 0;
let startedAtMs = 0;
let timerId;

function setStatus(value) {
  if (statusOutput) {
    statusOutput.value = value;
    statusOutput.textContent = value;
  }
}

function formatTime(seconds) {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainder = safeSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
}

function parseLength(value) {
  const match = value.trim().match(/^(\d{1,3}):([0-5]\d)$/);

  if (!match) {
    throw new Error('Length must use MM:SS, for example 02:30.');
  }

  const minutes = Number(match[1]);
  const seconds = Number(match[2]);
  const total = minutes * 60 + seconds;

  if (total <= 0) {
    throw new Error('Length must be at least 00:01.');
  }

  return total;
}

function parseSeed(value) {
  const trimmed = value.trim();

  if (!/^\d+$/.test(trimmed)) {
    throw new Error('Seed must contain decimal digits only.');
  }

  return BigInt(trimmed);
}

function randomSeed() {
  const values = new Uint32Array(2);
  crypto.getRandomValues(values);
  return (BigInt(values[0]) << 32n) | BigInt(values[1]);
}

function readTrackSettings() {
  activeSeed = parseSeed(seedInput.value);
  trackLengthSeconds = parseLength(lengthInput.value);
  lengthInput.value = formatTime(trackLengthSeconds);
  seedInput.value = activeSeed.toString();
  seedOutput.textContent = activeSeed.toString();
  updateTimeDisplay();
}

function setSettingsDisabled(disabled) {
  seedInput.disabled = disabled;
  lengthInput.disabled = disabled;
}

function currentElapsedSeconds() {
  if (startedAtMs === 0) {
    return elapsedBeforeStartMs / 1000;
  }

  return (elapsedBeforeStartMs + Date.now() - startedAtMs) / 1000;
}

function updateTimeDisplay() {
  timeOutput.textContent = `${formatTime(currentElapsedSeconds())} / ${formatTime(trackLengthSeconds)}`;
}

function startTimer() {
  stopTimer();
  startedAtMs = Date.now();
  timerId = window.setInterval(async () => {
    updateTimeDisplay();

    if (currentElapsedSeconds() >= trackLengthSeconds) {
      await stopTrack('Track complete.');
    }
  }, 250);
}

function pauseTimer() {
  if (startedAtMs !== 0) {
    elapsedBeforeStartMs += Date.now() - startedAtMs;
    startedAtMs = 0;
  }

  stopTimer();
  updateTimeDisplay();
}

function resetTimer() {
  stopTimer();
  elapsedBeforeStartMs = 0;
  startedAtMs = 0;
  updateTimeDisplay();
}

function stopTimer() {
  if (timerId !== undefined) {
    window.clearInterval(timerId);
    timerId = undefined;
  }
}

function updateSnapshot() {
  if (!player) {
    seedOutput.textContent = activeSeed.toString();
    updateTimeDisplay();
    return;
  }

  const snapshot = player.getSnapshot();
  sectionOutput.textContent = snapshot.currentSection;
  barOutput.textContent = String(snapshot.currentBar);
  seedOutput.textContent = snapshot.seed;
  updateTimeDisplay();
}

async function ensurePlayer() {
  if (player) {
    return player;
  }

  // Memoise the in-flight creation promise (not just the eventual player) so
  // two overlapping calls, such as a fast double click, await the same
  // AlgoWasmPlayer instead of each starting their own AudioContext/engine and
  // orphaning one of them.
  if (!playerPromise) {
    playerPromise = createPlayer().catch(error => {
      playerPromise = undefined;
      throw error;
    });
  }

  return playerPromise;
}

async function createPlayer() {
  setStatus('Loading engine...');
  const created = await AlgoWasmPlayer.create({
    wasmUrl: new URL('./vendor/algo-wasm/algo_wasm.wasm', import.meta.url),
    workletUrl: new URL('./vendor/algo-wasm/worklet/algo-worklet.js', import.meta.url),
    profile: profileFactories[activeProfileId](),
    seed: activeSeed,
    volume: 0.82
  });
  player = created;
  engineSeed = activeSeed;

  created.on('bar', updateSnapshot);
  created.on('section-change', updateSnapshot);
  created.on('error', error => setStatus(error.message));
  updateSnapshot();
  setStatus('Loaded.');
  return created;
}

async function startTrack() {
  readTrackSettings();
  const activePlayer = await ensurePlayer();

  if (engineSeed !== activeSeed) {
    activePlayer.setSeed(activeSeed);
    engineSeed = activeSeed;
    resetTimer();
  }

  await activePlayer.start();
  startTimer();
  setSettingsDisabled(true);
  setStatus('Playing.');
  updateSnapshot();
}

async function stopTrack(status = 'Stopped.') {
  if (!player) {
    return;
  }

  await player.stop();
  resetTimer();
  setSettingsDisabled(false);
  setStatus(status);
  updateSnapshot();
}

startButton?.addEventListener('click', async () => {
  try {
    await startTrack();
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  }
});

pauseButton?.addEventListener('click', async () => {
  if (!player) {
    return;
  }

  await player.pause();
  pauseTimer();
  setSettingsDisabled(false);
  setStatus('Paused.');
  updateSnapshot();
});

stopButton?.addEventListener('click', async () => {
  await stopTrack();
});

randomButton?.addEventListener('click', async () => {
  try {
    activeSeed = randomSeed();
    seedInput.value = activeSeed.toString();
    resetTimer();
    await startTrack();
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  }
});

energyInput?.addEventListener('input', () => {
  player?.setEnergy(Number(energyInput.value));
});

intensityInput?.addEventListener('input', () => {
  player?.setIntensity(Number(intensityInput.value));
});

moodInput?.addEventListener('change', () => {
  player?.setMood(moodInput.value);
});

profileInput?.addEventListener('change', () => {
  activeProfileId = profileInput.value;
  player?.setProfile(profileFactories[activeProfileId]());
});

lengthInput?.addEventListener('change', () => {
  try {
    trackLengthSeconds = parseLength(lengthInput.value);
    lengthInput.value = formatTime(trackLengthSeconds);
    updateTimeDisplay();
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  }
});

seedInput?.addEventListener('change', () => {
  try {
    activeSeed = parseSeed(seedInput.value);
    seedInput.value = activeSeed.toString();
    seedOutput.textContent = activeSeed.toString();
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  }
});

updateSnapshot();
