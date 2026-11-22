/**
 * `new OpenSMT(options?)` assembly session.
 *
 * The constructor is synchronous and opens nothing: no network, no disk scan,
 * no window, no file. Native initialisation happens inside the async methods.
 */

import { SmtDocument } from './document.js';
import { OpenSMTError } from './errors.js';
import * as service from './service.js';
import type { ImportReport, OpenSMTCapabilities, OpenSMTOptions } from './types.js';

/** Assembly session. One instance owns the documents it opens. */
export class OpenSMT {
    /** BCP-47 tag used to pick the CLI locale. */
    readonly locale: string;
    private readonly documents = new Set<SmtDocument>();
    private closed = false;

    constructor(options: OpenSMTOptions = {}) {
        this.locale = options.locale ?? 'en-US';
    }

    /** Whether {@link dispose} has run. */
    get disposed(): boolean {
        return this.closed;
    }

    /** Analysis and I/O disclosure for the installed engine. */
    async capabilities(): Promise<OpenSMTCapabilities> {
        this.assertActive();
        return service.capabilities();
    }

    /** Creates an empty document at `path` and returns it. */
    async create(path: string): Promise<SmtDocument> {
        this.assertActive();
        assertPath(path);
        const document = new SmtDocument(service.create(path));
        this.documents.add(document);
        return document;
    }

    /** Opens the document at `path` and returns it. */
    async open(path: string): Promise<SmtDocument> {
        this.assertActive();
        assertPath(path);
        const document = new SmtDocument(service.open(path));
        this.documents.add(document);
        return document;
    }

    /** Imports a KiCad PCB into a new document at `target` and returns the report. */
    async import(source: string, target: string): Promise<ImportReport> {
        this.assertActive();
        assertPath(source);
        assertPath(target);
        return service.importKicad(source, target);
    }

    /** Closes every document the session still owns. Idempotent. */
    async dispose(): Promise<void> {
        if (this.closed) {
            return;
        }
        this.closed = true;
        for (const document of [...this.documents]) {
            document.close();
        }
        this.documents.clear();
    }

    private assertActive(): void {
        if (this.closed) {
            throw new OpenSMTError('session/disposed', 'the OpenSMT session has been disposed; create a new instance');
        }
    }
}

function assertPath(path: string): void {
    if (typeof path !== 'string' || path.length === 0) {
        throw new OpenSMTError('session/argument-invalid', '`path` must be a non-empty string', {
            details: { feature: 'session.path' },
        });
    }
}
