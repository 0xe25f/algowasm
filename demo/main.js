import { AlgoWasmPlayer, builtInProfiles } from './vendor/algo-wasm/index.js';

const startButton = document.querySelector('#start');
const pauseButton = document.querySelector('#pause');
const stopButton = document.querySelector('#stop');
const randomButton = document.querySelector('#random');
const shareButton = document.querySelector('#share');
const seedInput = document.querySelector('#seedInput');
const lengthInput = document.querySelector('#lengthInput');
const profileRow = document.querySelector('#profiles');
const moodRow = document.querySelector('#moods');
const energyInput = document.querySelector('#energy');
const intensityInput = document.querySelector('#intensity');
const energyValue = document.querySelector('#energyValue');
const intensityValue = document.querySelector('#intensityValue');
const statusOutput = document.querySelector('#status');
const timeOutput = document.querySelector('#time');
const sectionOutput = document.querySelector('#section');
const barOutput = document.querySelector('#bar');
const tempoOutput = document.querySelector('#tempo');
const canvas = document.querySelector('#visualiser');

// The style buttons are built from the profiles the package ships, and take
// their labels from each profile's own displayName, so this page cannot drift
// out of step with the library.
const profiles = new Map(
  Object.values(builtInProfiles).map(createProfile => [createProfile().id, createProfile])
);
const moods = ['dark', 'bright', 'tense', 'calm'];

// One lane per engine track, top to bottom. The colour groups drums, bass,
// harmony, and melody so the picture reads at a glance.
const lanes = [
  { id: 'kick', label: 'Kick', colour: '#fbbf24' },
  { id: 'snare', label: 'Snare', colour: '#fbbf24' },
  { id: 'closed_hat', label: 'Closed hat', colour: '#fde68a' },
  { id: 'open_hat', label: 'Open hat', colour: '#fde68a' },
  { id: 'percussion', label: 'Percussion', colour: '#fde68a' },
  { id: 'bass', label: 'Bass', colour: '#5eead4' },
  { id: 'chords', label: 'Chords', colour: '#a78bfa' },
  { id: 'pad', label: 'Pad', colour: '#a78bfa' },
  { id: 'lead', label: 'Lead', colour: '#f472b6' },
  { id: 'arp', label: 'Arp', colour: '#f472b6' },
  { id: 'fx', label: 'FX', colour: '#93c5fd' }
];
const laneIndex = new Map(lanes.map((lane, index) => [lane.id, index]));

/** Seconds of history the visualiser shows, right to left. */
const HISTORY_SECONDS = 4;
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

let player;
let playerPromise;
let activeSeed = 1995n;
let engineSeed;
let activeProfileId = 'amiga_house_95ish';
let activeMood = 'dark';
let trackLengthSeconds = 120;
let elapsedBeforeStartMs = 0;
let startedAtMs = 0;
let timerId;
let lastBatchKey = '';
let hits = [];
let beats = [];

function setStatus(value, state = 'idle') {
  statusOutput.textContent = value;
  statusOutput.dataset.state = state;
}

function showError(error) {
  setStatus(error instanceof Error ? error.message : String(error), 'error');
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

  const total = Number(match[1]) * 60 + Number(match[2]);

  if (total <= 0) {
    throw new Error('Length must be at least 00:01.');
  }

  return total;
}

function parseSeed(value) {
  const trimmed = value.trim();

  if (!/^\d{1,20}$/.test(trimmed)) {
    throw new Error('Seed must be a whole number of up to 20 digits.');
  }

  return BigInt.asUintN(64, BigInt(trimmed));
}

function randomSeed() {
  const values = new Uint32Array(2);
  crypto.getRandomValues(values);
  return (BigInt(values[0]) << 32n) | BigInt(values[1]);
}

/** Builds a row of toggle buttons where exactly one is pressed. */
function buildChips(row, options, isActive, onPick) {
  for (const { value, label } of options) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    button.dataset.value = value;
    button.setAttribute('aria-pressed', String(isActive(value)));
    button.addEventListener('click', () => {
      onPick(value);

      for (const other of row.querySelectorAll('button')) {
        other.setAttribute('aria-pressed', String(other.dataset.value === value));
      }
    });
    row.append(button);
  }
}

/** Reads the seed, style, and mood from a shared link's fragment. */
function readSharedSettings() {
  const params = new URLSearchParams(location.hash.slice(1));
  const seed = params.get('seed');
  const profile = params.get('profile');
  const mood = params.get('mood');

  // Values from a link are untrusted, so each is checked before it is used.
  if (seed && /^\d{1,20}$/.test(seed)) {
    activeSeed = BigInt.asUintN(64, BigInt(seed));
    seedInput.value = activeSeed.toString();
  }

  if (profile && profiles.has(profile)) {
    activeProfileId = profile;
  }

  if (mood && moods.includes(mood)) {
    activeMood = mood;
  }
}

function shareUrl() {
  const params = new URLSearchParams({
    seed: activeSeed.toString(),
    profile: activeProfileId,
    mood: activeMood
  });
  return `${location.origin}${location.pathname}#${params}`;
}

function readTrackSettings() {
  activeSeed = parseSeed(seedInput.value);
  trackLengthSeconds = parseLength(lengthInput.value);
  lengthInput.value = formatTime(trackLengthSeconds);
  seedInput.value = activeSeed.toString();
  updateTimeDisplay();
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
  timerId = window.setInterval(() => {
    updateTimeDisplay();

    if (currentElapsedSeconds() >= trackLengthSeconds) {
      stopTrack('Track complete. Press Play to hear it again, or try a new song.').catch(showError);
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
    updateTimeDisplay();
    return;
  }

  const snapshot = player.getSnapshot();
  sectionOutput.textContent = snapshot.currentSection.replace('_', ' ');
  barOutput.textContent = String(snapshot.currentBar + 1);
  tempoOutput.textContent = `${snapshot.bpm} BPM`;
  updateTimeDisplay();
}

async function ensurePlayer() {
  if (player) {
    return player;
  }

  // Memoise the in-flight creation promise (not just the eventual player) so
  // two overlapping calls, such as a fast double click, await the same
  // AlgoWasmPlayer instead of each starting their own AudioContext and engine.
  if (!playerPromise) {
    playerPromise = createPlayer().catch(error => {
      playerPromise = undefined;
      throw error;
    });
  }

  return playerPromise;
}

async function createPlayer() {
  setStatus('Loading the engine...');
  const created = await AlgoWasmPlayer.create({
    wasmUrl: new URL('./vendor/algo-wasm/algo_wasm.wasm', import.meta.url),
    workletUrl: new URL('./vendor/algo-wasm/worklet/algo-worklet.js', import.meta.url),
    profile: profiles.get(activeProfileId)(),
    seed: activeSeed,
    volume: 0.82
  });
  created.setEnergy(Number(energyInput.value));
  created.setIntensity(Number(intensityInput.value));
  created.setMood(activeMood);
  player = created;
  engineSeed = activeSeed;

  created.on('bar', updateSnapshot);
  created.on('section-change', updateSnapshot);
  created.on('beat', event => beats.push({ at: performance.now(), downbeat: event.beat === 1 }));
  created.on('error', showError);
  updateSnapshot();
  return created;
}

/** Clears the visualiser, for when the song stops or restarts. */
function resetVisualiser() {
  hits = [];
  beats = [];
}

async function startTrack() {
  readTrackSettings();
  const activePlayer = await ensurePlayer();

  if (engineSeed !== activeSeed) {
    activePlayer.setSeed(activeSeed);
    engineSeed = activeSeed;
    resetTimer();
    resetVisualiser();
  }

  await activePlayer.start();
  startTimer();
  setStatus('Playing. Move the sliders or change the style while it plays.', 'playing');
  updateSnapshot();
}

async function stopTrack(status = 'Stopped.') {
  if (!player) {
    return;
  }

  await player.stop();
  resetTimer();
  resetVisualiser();
  setStatus(status);
  updateSnapshot();
}

/** Collects new note events from the player for the visualiser. */
function collectHits(now) {
  if (!player) {
    return;
  }

  const events = player.getCurrentEvents();

  if (events.length === 0) {
    return;
  }

  // The player replaces its batch of events about every 21 ms, and this runs
  // once per frame, so the same batch is often seen twice. A batch is
  // recognised by its size and its first and last sample positions. Comparing
  // batches, rather than keeping the newest sample seen, still works after
  // the song restarts and its sample positions begin again from zero.
  const key = `${events.length}:${events[0].sample}:${events.at(-1).sample}`;

  if (key === lastBatchKey) {
    return;
  }

  lastBatchKey = key;

  for (const event of events) {
    if (event.type === 'note-on') {
      hits.push({ lane: laneIndex.get(event.track) ?? 0, at: now, velocity: event.velocity });
    }
  }
}

function drawVisualiser() {
  const now = performance.now();
  collectHits(now);

  const context = canvas.getContext('2d');
  const ratio = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;

  // Match the backing store to the displayed size, so the picture stays sharp
  // on high-density screens and after the window is resized.
  if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
  }

  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.clearRect(0, 0, width, height);

  const labelWidth = Math.min(96, width * 0.24);
  const laneHeight = height / lanes.length;
  const historyMs = HISTORY_SECONDS * 1000;
  const timeline = width - labelWidth;
  const still = reducedMotion.matches;

  hits = hits.filter(hit => now - hit.at < historyMs);
  beats = beats.filter(beat => now - beat.at < historyMs);

  if (!still) {
    for (const beat of beats) {
      const x = width - ((now - beat.at) / historyMs) * timeline;
      context.fillStyle = beat.downbeat ? 'rgba(226, 232, 245, 0.22)' : 'rgba(226, 232, 245, 0.08)';
      context.fillRect(x, 0, 1, height);
    }
  }

  context.font = '12px ui-sans-serif, system-ui, sans-serif';
  context.textBaseline = 'middle';

  lanes.forEach((lane, index) => {
    const top = index * laneHeight;
    const latest = hits.findLast(hit => hit.lane === index);
    // Each lane's label lights up when its track plays and fades over a
    // quarter of a second.
    const glow = latest ? Math.max(0, 1 - (now - latest.at) / 250) : 0;

    context.fillStyle = index % 2 === 0 ? 'rgba(226, 232, 245, 0.025)' : 'transparent';
    context.fillRect(0, top, width, laneHeight);
    context.globalAlpha = 0.45 + glow * 0.55;
    context.fillStyle = lane.colour;
    context.fillRect(8, top + laneHeight / 2 - 3, 6, 6);
    context.fillStyle = '#e9eef7';
    context.fillText(lane.label, 22, top + laneHeight / 2);
    context.globalAlpha = 1;

    if (still) {
      // With reduced motion, show a steady level bar instead of scrolling notes.
      context.globalAlpha = glow;
      context.fillStyle = lane.colour;
      context.fillRect(labelWidth, top + 4, timeline * 0.5, laneHeight - 8);
      context.globalAlpha = 1;
    }
  });

  if (!still) {
    for (const hit of hits) {
      const age = (now - hit.at) / historyMs;
      const x = width - age * timeline;
      const size = 4 + hit.velocity * 10;
      context.globalAlpha = Math.max(0.15, 1 - age);
      context.fillStyle = lanes[hit.lane].colour;
      context.fillRect(x - size, hit.lane * laneHeight + 4, size, laneHeight - 8);
    }

    context.globalAlpha = 1;
  }

  requestAnimationFrame(drawVisualiser);
}

startButton.addEventListener('click', () => {
  startTrack().catch(showError);
});

pauseButton.addEventListener('click', async () => {
  if (!player) {
    return;
  }

  try {
    await player.pause();
    pauseTimer();
    setStatus('Paused. Press Play to carry on.');
    updateSnapshot();
  } catch (error) {
    showError(error);
  }
});

stopButton.addEventListener('click', () => {
  stopTrack().catch(showError);
});

randomButton.addEventListener('click', () => {
  activeSeed = randomSeed();
  seedInput.value = activeSeed.toString();
  resetTimer();
  startTrack().catch(showError);
});

shareButton.addEventListener('click', async () => {
  try {
    readTrackSettings();
    const url = shareUrl();
    history.replaceState(null, '', url);
    await navigator.clipboard.writeText(url);
    setStatus('Link copied. Anyone who opens it hears this exact song.');
  } catch (error) {
    // Clipboard access can be refused. The address bar still holds the link.
    setStatus('Could not copy automatically. Copy the link from the address bar instead.');
  }
});

energyInput.addEventListener('input', () => {
  energyValue.textContent = Number(energyInput.value).toFixed(2);
  player?.setEnergy(Number(energyInput.value));
});

intensityInput.addEventListener('input', () => {
  intensityValue.textContent = Number(intensityInput.value).toFixed(2);
  player?.setIntensity(Number(intensityInput.value));
});

lengthInput.addEventListener('change', () => {
  try {
    trackLengthSeconds = parseLength(lengthInput.value);
    lengthInput.value = formatTime(trackLengthSeconds);
    updateTimeDisplay();
  } catch (error) {
    showError(error);
  }
});

seedInput.addEventListener('change', () => {
  try {
    activeSeed = parseSeed(seedInput.value);
    seedInput.value = activeSeed.toString();
  } catch (error) {
    showError(error);
  }
});

readSharedSettings();

buildChips(
  profileRow,
  [...profiles].map(([id, createProfile]) => ({ value: id, label: createProfile().displayName })),
  id => id === activeProfileId,
  id => {
    activeProfileId = id;
    // The song restarts from its first bar in the new style and keeps playing.
    player?.setProfile(profiles.get(id)());
    resetTimer();
    resetVisualiser();

    if (player?.getSnapshot().isPlaying) {
      startTimer();
    }
  }
);

buildChips(
  moodRow,
  moods.map(mood => ({ value: mood, label: mood[0].toUpperCase() + mood.slice(1) })),
  mood => mood === activeMood,
  mood => {
    activeMood = mood;
    player?.setMood(mood);
  }
);

updateSnapshot();
requestAnimationFrame(drawVisualiser);
