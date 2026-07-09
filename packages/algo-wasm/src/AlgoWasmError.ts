import type { AlgoWasmErrorCode } from './types.js';

export class AlgoWasmError extends Error {
  readonly code: AlgoWasmErrorCode;

  constructor(code: AlgoWasmErrorCode, message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'AlgoWasmError';
    this.code = code;
  }
}
