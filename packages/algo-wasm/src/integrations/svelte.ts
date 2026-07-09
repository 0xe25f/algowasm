import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';
import type { AlgoWasmSnapshot } from '../types.js';

export interface AlgoWasmReadableStore {
  subscribe(run: (snapshot: AlgoWasmSnapshot) => void): () => void;
}

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
