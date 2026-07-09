import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';
import type { AlgoWasmSnapshot } from '../types.js';

export interface AlgoWasmVueBridge {
  snapshot: AlgoWasmSnapshot;
  dispose(): void;
}

export function createAlgoWasmVueBridge(
  player: AlgoWasmPlayer,
  assign: (snapshot: AlgoWasmSnapshot) => void
): AlgoWasmVueBridge {
  const update = () => assign(player.getSnapshot());
  const offBar = player.on('bar', update);
  const offStarted = player.on('started', update);
  const offStopped = player.on('stopped', update);

  update();

  return {
    snapshot: player.getSnapshot(),
    dispose() {
      offBar();
      offStarted();
      offStopped();
    }
  };
}
