import Phaser from 'phaser';
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';

const seedInput = document.querySelector<HTMLInputElement>('#seedInput');
const lengthInput = document.querySelector<HTMLInputElement>('#lengthInput');
const profileInput = document.querySelector<HTMLSelectElement>('#profileInput');
const startButton = document.querySelector<HTMLButtonElement>('#startTrack');
const randomButton = document.querySelector<HTMLButtonElement>('#randomTrack');
const statusOutput = document.querySelector<HTMLDivElement>('#status');

// The option labels always match each profile's own displayName, so this
// example cannot drift out of sync with the profiles the package ships.
const profileFactories = {
  amiga_house_95ish: builtInProfiles.amigaHouse95ish,
  downtempo_breakbeat: builtInProfiles.downtempoBreakbeat,
  dub_deep_house: builtInProfiles.dubDeepHouse
} as const;

type ProfileId = keyof typeof profileFactories;

for (const [id, factory] of Object.entries(profileFactories)) {
  const option = document.createElement('option');
  option.value = id;
  option.textContent = factory().displayName;
  profileInput?.append(option);
}

let music: AlgoWasmPlayer | undefined;
let musicPromise: Promise<AlgoWasmPlayer> | undefined;
let activeSeed = '1234';
let engineSeed = '';
let activeProfileId: ProfileId = (profileInput?.value as ProfileId) ?? 'amiga_house_95ish';
let durationSeconds = 120;
let elapsedBeforeStartMs = 0;
let startedAtMs = 0;
let timerId: number | undefined;

class BootScene extends Phaser.Scene {
  private label?: Phaser.GameObjects.Text;

  constructor() {
    super('boot');
  }

  create() {
    this.label = this.add.text(24, 24, 'Use the controls above to start AlgoWASM', {
      fontFamily: 'monospace',
      fontSize: '18px',
      color: '#ffffff'
    });
  }

  update() {
    if (!music) {
      return;
    }

    const energy = 1 - this.input.activePointer.y / this.scale.height;
    music.setEnergy(Phaser.Math.Clamp(energy, 0, 1));
    this.label?.setText(`Energy follows pointer height\nSeed ${activeSeed}`);
  }
}

startButton?.addEventListener('click', async () => {
  try {
    await startTrack(false);
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  }
});

randomButton?.addEventListener('click', async () => {
  try {
    activeSeed = randomSeed().toString();

    if (seedInput) {
      seedInput.value = activeSeed;
    }

    resetTimer();
    await startTrack(true);
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  }
});

profileInput?.addEventListener('change', () => {
  activeProfileId = profileInput.value as ProfileId;
  music?.setProfile(profileFactories[activeProfileId]());
});

async function startTrack(forceNewSeed: boolean) {
  const nextSeed = parseSeed(seedInput?.value ?? activeSeed);
  durationSeconds = parseLength(lengthInput?.value ?? '02:00');
  activeSeed = nextSeed;

  if (lengthInput) {
    lengthInput.value = formatTime(durationSeconds);
  }

  if (seedInput) {
    seedInput.value = activeSeed;
  }

  if (!music) {
    // Memoise the in-flight creation promise so overlapping calls (for
    // example a fast double click on Start) await the same AlgoWasmPlayer
    // instead of each creating their own AudioContext/engine and orphaning
    // one of them.
    if (!musicPromise) {
      musicPromise = AlgoWasmPlayer.create({
        wasmUrl: '/algo_wasm.wasm',
        workletUrl: '/algo-worklet.js',
        profile: profileFactories[activeProfileId](),
        seed: BigInt(activeSeed)
      }).catch(error => {
        musicPromise = undefined;
        throw error;
      });
    }

    music = await musicPromise;
    engineSeed = activeSeed;
  }

  if (forceNewSeed || engineSeed !== activeSeed) {
    music.setSeed(BigInt(activeSeed));
    engineSeed = activeSeed;
    resetTimer();
  }

  await music.start();
  setSettingsDisabled(true);
  startTimer();
  setStatus(`Time: ${formatTime(currentElapsedSeconds())} / ${formatTime(durationSeconds)}. Playing.`);
}

function startTimer() {
  stopTimer();
  startedAtMs = Date.now();
  timerId = window.setInterval(async () => {
    setStatus(`Time: ${formatTime(currentElapsedSeconds())} / ${formatTime(durationSeconds)}. Playing.`);

    if (currentElapsedSeconds() >= durationSeconds) {
      await music?.stop();
      resetTimer();
      setSettingsDisabled(false);
      setStatus(`Time: 00:00 / ${formatTime(durationSeconds)}. Track complete.`);
    }
  }, 250);
}

function resetTimer() {
  stopTimer();
  elapsedBeforeStartMs = 0;
  startedAtMs = 0;
}

function stopTimer() {
  if (timerId !== undefined) {
    window.clearInterval(timerId);
    timerId = undefined;
  }
}

function currentElapsedSeconds() {
  if (startedAtMs === 0) {
    return elapsedBeforeStartMs / 1000;
  }

  return (elapsedBeforeStartMs + Date.now() - startedAtMs) / 1000;
}

function setSettingsDisabled(disabled: boolean) {
  if (seedInput) {
    seedInput.disabled = disabled;
  }

  if (lengthInput) {
    lengthInput.disabled = disabled;
  }
}

function parseSeed(value: string) {
  const trimmed = value.trim();

  if (!/^\d+$/.test(trimmed)) {
    throw new Error('Seed must contain decimal digits only.');
  }

  return BigInt(trimmed).toString();
}

function parseLength(value: string) {
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

function formatTime(seconds: number) {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainder = safeSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
}

function randomSeed() {
  const values = new Uint32Array(2);
  crypto.getRandomValues(values);
  return (BigInt(values[0]) << 32n) | BigInt(values[1]);
}

function setStatus(value: string) {
  if (statusOutput) {
    statusOutput.textContent = value;
  }
}

new Phaser.Game({
  type: Phaser.AUTO,
  parent: 'game',
  width: 800,
  height: 450,
  backgroundColor: '#151a18',
  scene: [BootScene]
});
