/**
 * Path-backed document.
 *
 * Every editing method forwards to the native session, which writes the file
 * atomically, and stores the summary it returns so `revision`, `components` and
 * `feeders` always describe what is on disk.
 */

import { OpenSMTError } from './errors.js';
import * as service from './service.js';
import type {
    BalanceReport,
    Board,
    BoardRequest,
    ComponentRequest,
    DocumentSummary,
    ExportFormat,
    ExportReport,
    FeederReport,
    FeederRequest,
    Panel,
    PanelRequest,
    PlacementReport,
    ReflowProfileRequest,
    ReflowReport,
    Report,
    ValidationReport,
} from './types.js';

/** One open document. */
export class SmtDocument {
    private state: DocumentSummary;
    private closed = false;

    constructor(summary: DocumentSummary) {
        this.state = summary;
    }

    /** Document identity, derived from the file stem. */
    get id(): string {
        return this.state.id;
    }

    /** Path of the document file. */
    get path(): string {
        return this.state.path;
    }

    /** Committed revision. */
    get revision(): number {
        return this.state.revision;
    }

    /** Declared unit system. */
    get units(): string {
        return this.state.units;
    }

    /** Current board. */
    get board(): Board {
        return this.state.board;
    }

    /** Current panel array. */
    get panel(): Panel {
        return this.state.panel;
    }

    /** Number of components. */
    get components(): number {
        return this.state.components;
    }

    /** Number of feeders. */
    get feeders(): number {
        return this.state.feeders;
    }

    /** Replaces the board. */
    async setBoard(request: BoardRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.setBoard(this.state.path, request);
        return this.state;
    }

    /** Replaces the panel array. */
    async setPanel(request: PanelRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.setPanel(this.state.path, request);
        return this.state;
    }

    /** Adds a component. */
    async addComponent(request: ComponentRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.addComponent(this.state.path, request);
        return this.state;
    }

    /** Removes a component. */
    async removeComponent(refdes: string): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.removeComponent(this.state.path, refdes);
        return this.state;
    }

    /** Adds a feeder. */
    async addFeeder(request: FeederRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.addFeeder(this.state.path, request);
        return this.state;
    }

    /** Removes a feeder. */
    async removeFeeder(id: string): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.removeFeeder(this.state.path, id);
        return this.state;
    }

    /** Replaces the reflow profile. */
    async setProfile(request: ReflowProfileRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.setProfile(this.state.path, request);
        return this.state;
    }

    /** Validates the document. */
    async validate(): Promise<ValidationReport> {
        this.assertOpen();
        return service.validate(this.state.path);
    }

    /** Returns the placement program. */
    async placement(): Promise<PlacementReport> {
        this.assertOpen();
        return service.placement(this.state.path);
    }

    /** Returns the feeder setup. */
    async feederSetup(): Promise<FeederReport> {
        this.assertOpen();
        return service.feeders(this.state.path);
    }

    /** Returns the multi-machine line balance. */
    async balance(machines: number): Promise<BalanceReport> {
        this.assertOpen();
        return service.balance(this.state.path, machines);
    }

    /** Returns the reflow evaluation. */
    async reflow(): Promise<ReflowReport> {
        this.assertOpen();
        return service.reflow(this.state.path);
    }

    /** Exports the document as `program`, `feeders`, `json` or `md`. */
    async exportDocument(format: ExportFormat, target: string): Promise<ExportReport> {
        this.assertOpen();
        return service.exportDocument(this.state.path, format, target);
    }

    /** Returns the full report. */
    async report(): Promise<Report> {
        this.assertOpen();
        return service.report(this.state.path);
    }

    /** Writes the document in place, or to `target` when supplied. */
    async save(target?: string): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.save(this.state.path, target);
        return this.state;
    }

    /** Marks the document closed. Idempotent. */
    close(): void {
        this.closed = true;
    }

    private assertOpen(): void {
        if (this.closed) {
            throw new OpenSMTError('session/disposed', 'the document has been closed; open it again');
        }
    }
}
