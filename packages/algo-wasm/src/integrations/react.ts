import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';
import type { AlgoWasmSnapshot } from '../types.js';

export interface AlgoWasmExternalStore {
  getSnapshot(): AlgoWasmSnapshot;
  subscribe(listener: () => void): () => void;
}

export function createAlgoWasmExternalStore(player: AlgoWasmPlayer): AlgoWasmExternalStore {
  const listeners = new Set<() => void>();
  const notify = () => {
    for (const listener of listeners) {
      listener();
    }
  };
  const offBar = player.on('bar', notify);
  const offStarted = player.on('started', notify);
  const offStopped = player.on('stopped', notify);

  return {
    getSnapshot() {
      return player.getSnapshot();
    },
    subscribe(listener) {
      listeners.add(listener);

      return () => {
        listeners.delete(listener);

        if (listeners.size === 0) {
          offBar();
          offStarted();
          offStopped();
        }
      };
    }
  };
}
