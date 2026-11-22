/**
 * Public surface of `@wae/open-smt`.
 *
 * The Rust session (`smt-model`'s `OpenSmt`) is the core: it owns document
 * editing, feeder allocation, placement sequencing and reflow evaluation. This
 * package is the typed wrapper and the CLI is a thin assembly over it.
 */

export { SmtDocument } from './document.js';
export { OpenSMT } from './open-smt.js';
export { NotImplementedError, OpenSMTError, errorCodes, isOpenSMTError } from './errors.js';
export type { OpenSMTErrorCode, OpenSMTErrorDetails, OpenSMTErrorOptions } from './errors.js';
export type * from './types.js';
