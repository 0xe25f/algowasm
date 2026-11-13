import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';
import type { AlgoWasmSnapshot } from '../types.js';

/** A store in the shape React's `useSyncExternalStore` expects. */
export interface AlgoWasmExternalStore {
  getSnapshot(): AlgoWasmSnapshot;
  subscribe(listener: () => void): () => void;
}

/**
 * Wraps a player as an external store that notifies on each bar, start, and
 * stop. Pass `subscribe` and `getSnapshot` to `useSyncExternalStore`.
 *
 * The player listeners are removed when the last subscriber unsubscribes.
 */
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
