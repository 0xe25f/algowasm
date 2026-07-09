import type {
  AlgoWasmEventHandler,
  AlgoWasmEventName,
  AlgoWasmEventPayloads
} from './types.js';

export class AlgoWasmEventEmitter {
  private readonly handlers = new Map<AlgoWasmEventName, Set<(payload: unknown) => void>>();

  on<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): () => void {
    const handlers = this.handlers.get(eventName) ?? new Set<(payload: unknown) => void>();
    handlers.add(handler as (payload: unknown) => void);
    this.handlers.set(eventName, handlers);

    return () => this.off(eventName, handler);
  }

  off<Name extends AlgoWasmEventName>(
    eventName: Name,
    handler: AlgoWasmEventHandler<Name>
  ): void {
    this.handlers.get(eventName)?.delete(handler as (payload: unknown) => void);
  }

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

  clear(): void {
    this.handlers.clear();
  }
}
