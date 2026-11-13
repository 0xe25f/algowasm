import type {
  AlgoWasmEventHandler,
  AlgoWasmEventName,
  AlgoWasmEventPayloads
} from './types.js';

/** A small typed event emitter used by `AlgoWasmPlayer`. */
export class AlgoWasmEventEmitter {
  private readonly handlers = new Map<AlgoWasmEventName, Set<(payload: unknown) => void>>();

  /** Adds a handler and returns a function that removes it again. */
  on<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): () => void {
    const handlers = this.handlers.get(eventName) ?? new Set<(payload: unknown) => void>();
    handlers.add(handler as (payload: unknown) => void);
    this.handlers.set(eventName, handlers);

    return () => this.off(eventName, handler);
  }

  /** Removes a handler. Removing one that was never added does nothing. */
  off<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): void {
    this.handlers.get(eventName)?.delete(handler as (payload: unknown) => void);
  }

  /** Calls every handler for an event, in the order they were added. */
  emit<Name extends AlgoWasmEventName>(
    eventName: Name,
    payload: AlgoWasmEventPayloads[Name]
  ): void {
    const handlers = this.handlers.get(eventName);

    if (!handlers) {
      return;
    }

    for (const handler of handlers) {
      handler(payload);
    }
  }

  /** Removes every handler for every event. */
  clear(): void {
    this.handlers.clear();
  }
}
