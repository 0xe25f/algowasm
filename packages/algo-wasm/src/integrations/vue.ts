import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';
import type { AlgoWasmSnapshot } from '../types.js';

/** The handle returned by `createAlgoWasmVueBridge`. */
export interface AlgoWasmVueBridge {
  snapshot: AlgoWasmSnapshot;
  dispose(): void;
}

/**
 * Calls `assign` with a fresh snapshot now and on each bar, start, and stop,
 * so a Vue ref can mirror the player. Call `dispose` when the component
 * unmounts to remove the listeners.
 */
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
