import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';

export interface PhaserLikeScene {
  registry?: {
    get(key: string): unknown;
  };
  events?: {
    once(eventName: string, handler: () => void): void;
  };
}

export function bindAlgoWasmToPhaserDanger(
  scene: PhaserLikeScene,
  player: AlgoWasmPlayer,
  registryKey = 'danger'
): () => void {
  let disposed = false;

  const update = () => {
    if (disposed) {
      return;
    }

    const value = scene.registry?.get(registryKey);
    player.setEnergy(typeof value === 'number' ? value : 0);
    requestAnimationFrame(update);
  };

  update();

  return () => {
    disposed = true;
  };
}
