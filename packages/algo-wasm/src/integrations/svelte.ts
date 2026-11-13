import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';
import type { AlgoWasmSnapshot } from '../types.js';

/** A store that follows Svelte's readable store contract. */
export interface AlgoWasmReadableStore {
  subscribe(run: (snapshot: AlgoWasmSnapshot) => void): () => void;
}

/**
 * Wraps a player as a Svelte readable store of snapshots, updated on each
 * bar, start, and stop. Unsubscribing removes that subscriber's listeners.
 */
export function readableAlgoWasmPlayer(player: AlgoWasmPlayer): AlgoWasmReadableStore {
  return {
    subscribe(run) {
      run(player.getSnapshot());
      const update = () => run(player.getSnapshot());
      const offBar = player.on('bar', update);
      const offStarted = player.on('started', update);
      const offStopped = player.on('stopped', update);

      return () => {
        offBar();
        offStarted();
        offStopped();
      };
    }
  };
}
