import type { AlgoWasmErrorCode } from './types.js';

/**
 * The error type thrown and emitted by AlgoWASM. Check `code` to tell the
 * kinds of failure apart; `cause` holds the underlying error when there is one.
 */
export class AlgoWasmError extends Error {
  /** A stable identifier for the kind of failure. */
  readonly code: AlgoWasmErrorCode;

  /** Creates an error with a stable code, a readable message, and an optional cause. */
  constructor(code: AlgoWasmErrorCode, message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'AlgoWasmError';
    this.code = code;
  }
}
