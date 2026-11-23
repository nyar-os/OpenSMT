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
    BomCheckReport,
    ChangeoverReport,
    ComponentRequest,
    ComponentUpdateRequest,
    ConsumptionReport,
    DocumentSummary,
    DualLaneReport,
    DualLaneStrategy,
    ExportFormat,
    ExportReport,
    FeederLoadingReport,
    FeederReport,
    FeederRequest,
    FeederUpdateRequest,
    FirstArticleReport,
    LanesReport,
    Panel,
    PanelFitReport,
    PanelFitRequest,
    PanelMapReport,
    PanelRequest,
    PlacementReport,
    ReflowProfileRequest,
    ReflowReport,
    ReflowWindowLimits,
    ReflowWindowReport,
    Report,
    StationReport,
    TaktOptions,
    TaktReport,
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

    /** Applies a partial edit to a placed component. */
    async updateComponent(refdes: string, request: ComponentUpdateRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.updateComponent(this.state.path, refdes, request);
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

    /** Applies a partial edit to a feeder. */
    async updateFeeder(id: string, request: FeederUpdateRequest): Promise<DocumentSummary> {
        this.assertOpen();
        this.state = service.updateFeeder(this.state.path, id, request);
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

    /** Returns the placement program, optionally improved with 2-opt. */
    async placement(optimize = false): Promise<PlacementReport> {
        this.assertOpen();
        return service.placement(this.state.path, optimize);
    }

    /** Returns the feeder setup. */
    async feederSetup(): Promise<FeederReport> {
        this.assertOpen();
        return service.feeders(this.state.path);
    }

    /** Returns the read-only feeder loading report. */
    async feederReport(): Promise<FeederLoadingReport> {
        this.assertOpen();
        return service.feederReport(this.state.path);
    }

    /** Returns the read-only panel fit report for a maximum panel size. */
    async panelFit(request: PanelFitRequest): Promise<PanelFitReport> {
        this.assertOpen();
        return service.panelFit(this.state.path, request);
    }

    /** Returns the read-only single-board to panel coordinate map. */
    async panelMap(): Promise<PanelMapReport> {
        this.assertOpen();
        return service.panelMap(this.state.path);
    }

    /** Returns the read-only first-article inspection sheet. */
    async firstArticle(): Promise<FirstArticleReport> {
        this.assertOpen();
        return service.firstArticle(this.state.path);
    }

    /** Returns the read-only BOM cross-check against the BOM CSV at `bomPath`. */
    async bomCheck(bomPath: string): Promise<BomCheckReport> {
        this.assertOpen();
        return service.bomCheck(this.state.path, bomPath);
    }

    /** Returns the multi-machine line balance, optionally improved per machine. */
    async balance(machines: number, optimize = false): Promise<BalanceReport> {
        this.assertOpen();
        return service.balance(this.state.path, machines, optimize);
    }

    /** Returns the dual-lane schedule, optionally with another strategy and swaps. */
    async dualLane(strategy: DualLaneStrategy = 'greedy', improve = false): Promise<DualLaneReport> {
        this.assertOpen();
        return service.dualLane(this.state.path, strategy, improve);
    }

    /** Returns the multi-lane schedule, optionally with another strategy and swaps. */
    async lanes(laneCount: number, strategy: DualLaneStrategy = 'greedy', improve = false): Promise<LanesReport> {
        this.assertOpen();
        return service.lanes(this.state.path, laneCount, strategy, improve);
    }

    /** Returns the reflow evaluation. */
    async reflow(): Promise<ReflowReport> {
        this.assertOpen();
        return service.reflow(this.state.path);
    }

    /** Returns the read-only reflow process-window report against the supplied limits. */
    async reflowWindow(limits: ReflowWindowLimits = {}): Promise<ReflowWindowReport> {
        this.assertOpen();
        return service.reflowWindow(this.state.path, limits);
    }

    /** Returns the read-only feeder consumption plan for `boards` boards. */
    async consumption(boards: number): Promise<ConsumptionReport> {
        this.assertOpen();
        return service.consumption(this.state.path, boards);
    }

    /**
     * Returns the read-only cycle time and throughput estimate.
     *
     * Omitted option fields keep their machine-model defaults.
     */
    async takt(options: TaktOptions = {}): Promise<TaktReport> {
        this.assertOpen();
        return service.takt(this.state.path, options);
    }

    /**
     * Returns the read-only feeder station assignment along the tape bank.
     *
     * An omitted `slotPitch` keeps the default distance between neighbouring
     * slots.
     */
    async feederOptimize(slotPitch?: number): Promise<StationReport> {
        this.assertOpen();
        return service.feederOptimize(this.state.path, slotPitch);
    }

    /**
     * Returns the read-only feeder changeover sequence to the document at
     * `nextPath`.
     *
     * An omitted `feederChangeSeconds` keeps the default time per reel change.
     */
    async changeover(nextPath: string, feederChangeSeconds?: number): Promise<ChangeoverReport> {
        this.assertOpen();
        return service.changeover(this.state.path, nextPath, feederChangeSeconds);
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
