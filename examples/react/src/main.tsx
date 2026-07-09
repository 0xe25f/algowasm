import React, { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { AlgoWasmPlayer, builtInProfiles } from '@algo-wasm/algo-wasm';
import type { AlgoWasmSnapshot } from '@algo-wasm/algo-wasm';
import './styles.css';

// The option labels always match each profile's own displayName, so this
// example cannot drift out of sync with the profiles the package ships.
const profileFactories = {
  amiga_house_95ish: builtInProfiles.amigaHouse95ish,
  downtempo_breakbeat: builtInProfiles.downtempoBreakbeat,
  dub_deep_house: builtInProfiles.dubDeepHouse
} as const;

type ProfileId = keyof typeof profileFactories;

const profileOptions = (Object.keys(profileFactories) as ProfileId[]).map(id => ({
  id,
  label: profileFactories[id]().displayName
}));

function App() {
  const playerRef = useRef<AlgoWasmPlayer | null>(null);
  const playerPromiseRef = useRef<Promise<AlgoWasmPlayer> | null>(null);
  const timerRef = useRef<number | undefined>(undefined);
  const startedAtRef = useRef(0);
  const elapsedBeforeStartRef = useRef(0);
  const activeSeedRef = useRef('1995');
  const durationSecondsRef = useRef(120);
  const [snapshot, setSnapshot] = useState<AlgoWasmSnapshot | null>(null);
  const [status, setStatus] = useState('Ready.');
  const [seed, setSeed] = useState('1995');
  const [length, setLength] = useState('02:00');
  const [profileId, setProfileId] = useState<ProfileId>('amiga_house_95ish');
  const [elapsed, setElapsed] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);

  useEffect(() => {
    return () => {
      stopTimer(timerRef);
      void playerRef.current?.destroy();
    };
  }, []);

  async function ensurePlayer(trackSeed: string) {
    if (!playerRef.current) {
      // Memoise the in-flight creation promise so overlapping calls (for
      // example a fast double click on Start) await the same
      // AlgoWasmPlayer instead of each creating their own
      // AudioContext/engine and orphaning one of them.
      if (!playerPromiseRef.current) {
        playerPromiseRef.current = AlgoWasmPlayer.create({
          wasmUrl: '/algo_wasm.wasm',
          workletUrl: '/algo-worklet.js',
          profile: profileFactories[profileId](),
          seed: BigInt(trackSeed)
        })
          .then(created => {
            created.on('bar', () => setSnapshot(playerRef.current?.getSnapshot() ?? null));
            playerRef.current = created;
            return created;
          })
          .catch(error => {
            playerPromiseRef.current = null;
            throw error;
          });
      }

      playerRef.current = await playerPromiseRef.current;
    }

    if (activeSeedRef.current !== trackSeed) {
      activeSeedRef.current = trackSeed;
      elapsedBeforeStartRef.current = 0;
      playerRef.current.setSeed(BigInt(trackSeed));
    }

    return playerRef.current;
  }

  function changeProfile(nextProfileId: ProfileId) {
    setProfileId(nextProfileId);
    playerRef.current?.setProfile(profileFactories[nextProfileId]());
  }

  async function toggle() {
    try {
      if (isPlaying) {
        await playerRef.current?.pause();
        pauseTimer();
        setIsPlaying(false);
        setStatus('Paused.');
        return;
      }

      const cleanSeed = parseSeed(seed);
      const duration = parseLength(length);
      setSeed(cleanSeed);
      setLength(formatTime(duration));
      durationSecondsRef.current = duration;

      const player = await ensurePlayer(cleanSeed);
      await player.start();
      startTimer();
      setIsPlaying(true);
      setStatus('Playing.');
      setSnapshot(player.getSnapshot());
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    }
  }

  async function stop() {
    await playerRef.current?.stop();
    stopTimer(timerRef);
    startedAtRef.current = 0;
    elapsedBeforeStartRef.current = 0;
    setElapsed(0);
    setIsPlaying(false);
    setStatus('Stopped.');
    setSnapshot(playerRef.current?.getSnapshot() ?? null);
  }

  async function playRandom() {
    const nextSeed = randomSeed().toString();
    setSeed(nextSeed);
    elapsedBeforeStartRef.current = 0;
    startedAtRef.current = 0;
    setElapsed(0);
    activeSeedRef.current = '';

    try {
      const duration = parseLength(length);
      durationSecondsRef.current = duration;
      const player = await ensurePlayer(nextSeed);
      await player.start();
      startTimer();
      setIsPlaying(true);
      setStatus('Playing.');
      setSnapshot(player.getSnapshot());
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    }
  }

  function startTimer() {
    stopTimer(timerRef);
    startedAtRef.current = Date.now();
    timerRef.current = window.setInterval(() => {
      const nextElapsed = currentElapsedSeconds(startedAtRef, elapsedBeforeStartRef);
      setElapsed(nextElapsed);

      if (nextElapsed >= durationSecondsRef.current) {
        void stop().then(() => setStatus('Track complete.'));
      }
    }, 250);
  }

  function pauseTimer() {
    if (startedAtRef.current !== 0) {
      elapsedBeforeStartRef.current += Date.now() - startedAtRef.current;
      startedAtRef.current = 0;
    }

    stopTimer(timerRef);
    setElapsed(currentElapsedSeconds(startedAtRef, elapsedBeforeStartRef));
  }

  return (
    <main className="shell">
      <section className="panel">
        <h1>AlgoWASM React</h1>
        <div className="controls">
          <label>
            Seed
            <input
              value={seed}
              disabled={isPlaying}
              inputMode="numeric"
              onChange={event => setSeed(event.currentTarget.value)}
            />
          </label>
          <label>
            Length
            <input
              value={length}
              disabled={isPlaying}
              inputMode="numeric"
              onChange={event => setLength(event.currentTarget.value)}
            />
          </label>
          <label>
            Profile
            <select
              value={profileId}
              onChange={event => changeProfile(event.currentTarget.value as ProfileId)}
            >
              {profileOptions.map(option => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="transport">
          <button type="button" onClick={toggle}>
            {isPlaying ? 'Pause Music' : 'Start Music'}
          </button>
          <button type="button" onClick={stop}>
            Stop
          </button>
          <button type="button" onClick={playRandom}>
            New Random
          </button>
        </div>
        <p>{status}</p>
        <p>
          Time: {formatTime(elapsed)} / {formatTime(durationSecondsRef.current)}
        </p>
        <p>Section: {snapshot?.currentSection ?? 'intro'}</p>
        <p>Seed: {snapshot?.seed ?? activeSeedRef.current}</p>
      </section>
    </main>
  );
}

function parseSeed(value: string): string {
  const trimmed = value.trim();

  if (!/^\d+$/.test(trimmed)) {
    throw new Error('Seed must contain decimal digits only.');
  }

  return BigInt(trimmed).toString();
}

function parseLength(value: string): number {
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

function formatTime(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainder = safeSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
}

function randomSeed(): bigint {
  const values = new Uint32Array(2);
  crypto.getRandomValues(values);
  return (BigInt(values[0]) << 32n) | BigInt(values[1]);
}

function currentElapsedSeconds(
  startedAtRef: React.MutableRefObject<number>,
  elapsedBeforeStartRef: React.MutableRefObject<number>
): number {
  if (startedAtRef.current === 0) {
    return elapsedBeforeStartRef.current / 1000;
  }

  return (elapsedBeforeStartRef.current + Date.now() - startedAtRef.current) / 1000;
}

function stopTimer(timerRef: React.MutableRefObject<number | undefined>) {
  if (timerRef.current !== undefined) {
    window.clearInterval(timerRef.current);
    timerRef.current = undefined;
  }
}

const root = createRoot(document.querySelector('#root') as HTMLElement);
root.render(<App />);
