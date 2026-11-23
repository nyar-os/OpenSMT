/**
 * Stable error surface of `@wae/open-smt`.
 *
 * Codes are contract, not prose: callers compare them, localize around them and
 * never parse the message.
 */

/** Every stable code this package may report. */
export const errorCodes = [
    'session/disposed',
    'session/argument-invalid',
    'session/no-document',
    'native/unavailable',
    'native/error',
    'file/exists',
    'io/read',
    'io/write',
    'document/parse',
    'document/serialize',
    'document/format',
    'document/version',
    'document/units',
    'import/parse',
    'import/outline-unsupported',
    'import/missing-column',
    'import/unknown-column',
    'import/duplicate-column',
    'import/invalid-header',
    'import/invalid-row',
    'import/invalid-value',
    'import/invalid-number',
    'import/invalid-side',
    'import/duplicate-refdes',
    'import/outside-board',
    'bom/parse',
    'bom/missing-column',
    'bom/unknown-column',
    'bom/duplicate-column',
    'bom/invalid-header',
    'bom/invalid-row',
    'bom/invalid-value',
    'bom/invalid-number',
    'bom/duplicate-refdes',
    'board/invalid',
    'panel/invalid',
    'balance/invalid-machines',
    'schedule/invalid-strategy',
    'schedule/invalid-lanes',
    'takt/invalid',
    'station/invalid',
    'changeover/invalid',
    'reflow/invalid',
    'consumption/invalid',
    'program/invalid',
    'component/invalid',
    'component/duplicate-refdes',
    'component/unknown-refdes',
    'component/outside-board',
    'component/unknown-side',
    'feeder/invalid',
    'feeder/duplicate-id',
    'feeder/unknown-id',
    'feeder/in-use',
    'feeder/missing',
    'feeder/capacity',
    'feeder/unused',
    'profile/invalid',
    'export/invalid',
    'cli/unknown-command',
    'cli/missing-argument',
    'cli/missing-option',
    'cli/missing-value',
    'cli/unknown-option',
    'cli/invalid-number',
    'cli/unknown-format',
    'capability/not-implemented',
] as const;

/** Union of {@link errorCodes}. */
export type OpenSMTErrorCode = (typeof errorCodes)[number];

/** Optional locators a failure can carry. */
export interface OpenSMTErrorDetails {
    /** Capability or field involved in the failure, e.g. `component.add.refdes`. */
    readonly feature?: string;
    /** Document path involved in the failure. */
    readonly path?: string;
}

/** Options accepted by {@link OpenSMTError}. */
export interface OpenSMTErrorOptions {
    readonly recoverable?: boolean;
    readonly details?: OpenSMTErrorDetails;
    readonly cause?: unknown;
}

/** Every failure thrown or rejected by this package. */
export class OpenSMTError extends Error {
    /** Stable, comparable code. */
    readonly code: OpenSMTErrorCode;
    /** Whether retrying with different input or state can succeed. */
    readonly recoverable: boolean;
    /** Optional locators (`feature`, `path`). */
    readonly details: OpenSMTErrorDetails;

    constructor(code: OpenSMTErrorCode, message: string, options: OpenSMTErrorOptions = {}) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = 'OpenSMTError';
        this.code = code;
        this.recoverable = options.recoverable ?? false;
        this.details = Object.freeze({ ...options.details });
    }
}

/** Reported when the optional native addon is absent or a route is unimplemented. */
export class NotImplementedError extends OpenSMTError {
    constructor(feature: string, message = `\`${feature}\` is not implemented yet`) {
        super('capability/not-implemented', message, { recoverable: false, details: { feature } });
        this.name = 'NotImplementedError';
    }
}

/** Narrows an unknown thrown value to {@link OpenSMTError}. */
export function isOpenSMTError(value: unknown): value is OpenSMTError {
    return value instanceof OpenSMTError;
}
