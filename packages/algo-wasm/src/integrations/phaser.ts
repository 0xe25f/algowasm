import type { AlgoWasmPlayer } from '../AlgoWasmPlayer.js';

/** The parts of a Phaser scene this integration reads. */
export interface PhaserLikeScene {
  registry?: {
    get(key: string): unknown;
  };
  events?: {
    once(eventName: string, handler: () => void): void;
  };
}

/**
 * Drives the player's energy from a number in the scene registry, read once
 * per animation frame. A missing or non-numeric value counts as 0.
 *
 * Returns a function that stops the updates. Call it when the scene shuts
 * down, or the loop keeps running.
 */
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
