/**
 * Orthogonal native operations.
 *
 * One function performs one operation and returns data; argument parsing, help
 * text and exit codes belong to `cli.ts`.
 */

import { NotImplementedError, OpenSMTError, errorCodes, type OpenSMTErrorCode } from './errors.js';
import { nativeBinding } from './native.js';
import type {
    BalanceReport,
    BoardRequest,
    ComponentRequest,
    ComponentUpdateRequest,
    DocumentSummary,
    DualLaneReport,
    DualLaneStrategy,
    ExportFormat,
    ExportReport,
    FeederLoadingReport,
    FeederReport,
    FeederRequest,
    FeederUpdateRequest,
    ImportReport,
    LanesReport,
    PanelFitReport,
    PanelFitRequest,
    PanelRequest,
    PlacementReport,
    ReflowProfileRequest,
    ReflowReport,
    Report,
    Side,
    ValidationReport,
} from './types.js';

const knownCodes = new Set<string>(errorCodes);

function binding(feature: string) {
    const native = nativeBinding();
    if (!native) {
        throw new NotImplementedError(feature);
    }
    return native;
}

function decode<T>(work: () => string): T {
    try {
        return JSON.parse(work()) as T;
    } catch (error) {
        throw translate(error);
    }
}

function translate(error: unknown): OpenSMTError {
    if (error instanceof OpenSMTError) {
        return error;
    }
    const message = error instanceof Error ? error.message : String(error);
    const separator = message.indexOf(': ');
    const code = separator > 0 ? message.slice(0, separator) : '';
    if (knownCodes.has(code)) {
        return new OpenSMTError(code as OpenSMTErrorCode, message.slice(separator + 2));
    }
    return new OpenSMTError('native/error', message, { cause: error });
}

/** Reads the version of the installed addon. */
export function version(): string {
    return decode<string>(() => binding('version').version());
}

/** Reads the capability matrix of the installed addon. */
export function capabilities() {
    const matrix = decode<{
        analysis: Record<string, 'supported' | 'unsupported'>;
        io: Record<string, 'supported' | 'unsupported'>;
        imports: Record<string, 'supported' | 'unsupported'>;
        limitations?: Record<string, string>;
    }>(() => binding('capabilities').capabilities());
    return {
        engine: { package: 'open-smt-napi', version: version() },
        analysis: matrix.analysis,
        io: matrix.io,
        imports: matrix.imports,
        limitations: matrix.limitations ?? {},
    };
}

/** Creates an empty document and returns its summary. */
export function create(path: string): DocumentSummary {
    return decode<DocumentSummary>(() => binding('create').create(path));
}

/** Opens a document and returns its summary. */
export function open(path: string): DocumentSummary {
    return decode<DocumentSummary>(() => binding('open').open(path));
}

/** Imports a KiCad PCB into a new document at `target`. */
export function importKicad(source: string, target: string): ImportReport {
    return decode<ImportReport>(() => binding('import').import(source, target));
}

/** Imports a generic CSV placement file into a new document at `target`. */
export function importCsv(source: string, target: string, defaultSide: Side = 'top'): ImportReport {
    return decode<ImportReport>(() => binding('import-csv').import_csv(source, target, defaultSide));
}

/** Writes a document to `target`, or in place when `target` is undefined. */
export function save(path: string, target?: string): DocumentSummary {
    return decode<DocumentSummary>(() =>
        target === undefined ? binding('save').save(path) : binding('save').save(path, target),
    );
}

/** Replaces the board and returns the updated summary. */
export function setBoard(path: string, request: BoardRequest): DocumentSummary {
    return decode<DocumentSummary>(() => binding('board.set').board_set(path, JSON.stringify(request)));
}

/** Replaces the panel array and returns the updated summary. */
export function setPanel(path: string, request: PanelRequest): DocumentSummary {
    return decode<DocumentSummary>(() => binding('panel.set').panel_set(path, JSON.stringify(request)));
}

/** Adds a component and returns the updated summary. */
export function addComponent(path: string, request: ComponentRequest): DocumentSummary {
    return decode<DocumentSummary>(() => binding('component.add').component_add(path, JSON.stringify(request)));
}

/** Applies a partial edit to a placed component and returns the updated summary. */
export function updateComponent(path: string, refdes: string, request: ComponentUpdateRequest): DocumentSummary {
    return decode<DocumentSummary>(() =>
        binding('component.update').component_update(path, JSON.stringify({ refdes, ...request })),
    );
}

/** Removes a component and returns the updated summary. */
export function removeComponent(path: string, refdes: string): DocumentSummary {
    return decode<DocumentSummary>(() =>
        binding('component.remove').component_remove(path, JSON.stringify({ refdes })),
    );
}

/** Adds a feeder and returns the updated summary. */
export function addFeeder(path: string, request: FeederRequest): DocumentSummary {
    return decode<DocumentSummary>(() => binding('feeder.add').feeder_add(path, JSON.stringify(request)));
}

/** Applies a partial edit to a feeder and returns the updated summary. */
export function updateFeeder(path: string, id: string, request: FeederUpdateRequest): DocumentSummary {
    return decode<DocumentSummary>(() =>
        binding('feeder.update').feeder_update(path, JSON.stringify({ id, ...request })),
    );
}

/** Removes a feeder and returns the updated summary. */
export function removeFeeder(path: string, id: string): DocumentSummary {
    return decode<DocumentSummary>(() => binding('feeder.remove').feeder_remove(path, JSON.stringify({ id })));
}

/** Replaces the reflow profile and returns the updated summary. */
export function setProfile(path: string, request: ReflowProfileRequest): DocumentSummary {
    return decode<DocumentSummary>(() => binding('profile.set').profile_set(path, JSON.stringify(request)));
}

/** Validates a document. */
export function validate(path: string): ValidationReport {
    return decode<ValidationReport>(() => binding('validate').validate(path));
}

/** Returns the placement program, optionally improved with 2-opt. */
export function placement(path: string, optimize = false): PlacementReport {
    return decode<PlacementReport>(() => binding('placement').placement(path, optimize));
}

/** Returns the feeder setup. */
export function feeders(path: string): FeederReport {
    return decode<FeederReport>(() => binding('feeders').feeders(path));
}

/** Returns the read-only feeder loading report. */
export function feederReport(path: string): FeederLoadingReport {
    return decode<FeederLoadingReport>(() => binding('feeder.report').feeder_report(path));
}

/** Returns the read-only panel fit report for a maximum panel size. */
export function panelFit(path: string, request: PanelFitRequest): PanelFitReport {
    return decode<PanelFitReport>(() => binding('panel.fit').panel_fit(path, JSON.stringify(request)));
}

/** Returns the multi-machine line balance, optionally improved per machine. */
export function balance(path: string, machines: number, optimize = false): BalanceReport {
    return decode<BalanceReport>(() => binding('balance').balance(path, machines, optimize));
}

/** Returns the dual-lane schedule, optionally with another strategy and swaps. */
export function dualLane(path: string, strategy: DualLaneStrategy = 'greedy', improve = false): DualLaneReport {
    return decode<DualLaneReport>(() => {
        const native = binding('dual-lane');
        if (strategy === 'greedy' && !improve) {
            return native.dual_lane(path);
        }
        if (!improve) {
            return native.dual_lane(path, strategy);
        }
        return native.dual_lane(path, strategy, improve);
    });
}

/** Returns the multi-lane schedule, optionally with another strategy and swaps. */
export function lanes(
    path: string,
    laneCount: number,
    strategy: DualLaneStrategy = 'greedy',
    improve = false,
): LanesReport {
    return decode<LanesReport>(() => binding('schedule-lanes').schedule_lanes(path, laneCount, strategy, improve));
}

/** Returns the reflow evaluation. */
export function reflow(path: string): ReflowReport {
    return decode<ReflowReport>(() => binding('reflow').reflow(path));
}

/** Exports a document as `program`, `feeders`, `json` or `md`. */
export function exportDocument(path: string, format: ExportFormat, target: string): ExportReport {
    return decode<ExportReport>(() => binding('export').export(path, format, target));
}

/** Returns the full report. */
export function report(path: string): Report {
    return decode<Report>(() => binding('report').report(path));
}
