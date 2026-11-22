/**
 * Declared types of `@wae/open-smt`.
 *
 * The shapes describe the real job: a board, an optional panel array, a
 * component list, a feeder setup and a reflow profile. Every editing call
 * returns the summary written to disk.
 */

/** The only unit system this build accepts: millimetres (temperatures are °C). */
export type Units = 'mm';

/** Board side a component is placed on. */
export type Side = 'top' | 'bottom';

/** Export formats `exportDocument()` can write. */
export type ExportFormat = 'program' | 'feeders' | 'json' | 'md';

/** Severity of a validation finding. */
export type Severity = 'error' | 'warning' | 'notice';

/** One rectangular board. */
export interface Board {
    readonly width: number;
    readonly depth: number;
    readonly thickness: number;
}

/** A panel array of boards. */
export interface Panel {
    readonly rows: number;
    readonly columns: number;
    readonly gapX: number;
    readonly gapY: number;
}

/** One placed component. */
export interface Component {
    readonly refdes: string;
    readonly package: string;
    readonly side: Side;
    readonly x: number;
    readonly y: number;
    /** Rotation in degrees, `0 … 360`. */
    readonly rotation: number;
    readonly height: number | null;
}

/** One feeder lane. */
export interface Feeder {
    readonly id: string;
    readonly package: string;
    readonly pitch: number;
    readonly capacity: number;
    readonly loaded: number;
}

/** One reflow zone. */
export interface Zone {
    readonly name: string;
    readonly length: number;
    readonly temperature: number;
}

/** Reflow profile. */
export interface ReflowProfile {
    /** Conveyor speed in millimetres per minute. */
    readonly conveyorSpeed: number;
    /** Alloy liquidus in degrees Celsius. */
    readonly liquidus: number;
    readonly zones: readonly Zone[];
}

/** Summary of one document as written to disk. */
export interface DocumentSummary {
    readonly id: string;
    readonly path: string;
    readonly units: string;
    readonly revision: number;
    readonly board: Board;
    readonly panel: Panel;
    readonly components: number;
    readonly feeders: number;
}

/** One validation finding with a stable code. */
export interface Issue {
    readonly code: string;
    readonly severity: Severity;
    readonly message: string;
}

/** Result of `validate()`. */
export interface ValidationReport {
    readonly ok: boolean;
    readonly issues: readonly Issue[];
}

/** One step of the placement program. */
export interface PlacementEntry {
    readonly index: number;
    readonly refdes: string;
    readonly package: string;
    readonly side: Side;
    readonly x: number;
    readonly y: number;
    readonly rotation: number;
    readonly feeder: string | null;
}

/** Usage of one package. */
export interface PackageUsage {
    readonly package: string;
    readonly components: number;
    readonly feeders: number;
}

/** Allocation of one feeder. */
export interface FeederUsage {
    readonly id: string;
    readonly package: string;
    readonly loaded: number;
    readonly capacity: number;
    readonly assigned: number;
    readonly remaining: number;
    readonly refdes: readonly string[];
}

/** Feeder setup report. */
export interface FeederReport {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    readonly packages: readonly PackageUsage[];
    readonly feeders: readonly FeederUsage[];
    readonly unassigned: readonly string[];
}

/** One suggested tape slot of a feeder loading report. */
export interface FeederSlot {
    /** Suggested slot number, starting at 1 in package order. */
    readonly slot: number;
    readonly package: string;
    /** Number of placements of this package. */
    readonly count: number;
    /** Reference designators in document order. */
    readonly refdes: readonly string[];
    /** Distinct board sides, sorted. */
    readonly sides: readonly Side[];
}

/** Read-only feeder loading report aggregated by package. */
export interface FeederLoadingReport {
    readonly format: string;
    readonly version: number;
    readonly totalPlacements: number;
    readonly uniquePackages: number;
    /** One suggested slot per package, in package order. */
    readonly slots: readonly FeederSlot[];
}

/** Read-only panel fit report for a maximum panel size. */
export interface PanelFitReport {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    /** Maximum panel width tested, in millimetres. */
    readonly maxWidth: number;
    /** Maximum panel depth tested, in millimetres. */
    readonly maxDepth: number;
    /** Column gap used by the fit, in millimetres. */
    readonly gapX: number;
    /** Row gap used by the fit, in millimetres. */
    readonly gapY: number;
    /** Board width, in millimetres. */
    readonly width: number;
    /** Board depth, in millimetres. */
    readonly depth: number;
    /** Number of board rows that fit. */
    readonly rows: number;
    /** Number of board columns that fit. */
    readonly columns: number;
    /** Number of boards on the fitted panel. */
    readonly boardCount: number;
    /** Width of the fitted panel including column gaps. */
    readonly panelWidth: number;
    /** Depth of the fitted panel including row gaps. */
    readonly panelDepth: number;
    /** Whether at least one board fits. */
    readonly fits: boolean;
    /** Unused width when a board fits; zero otherwise. */
    readonly wastedWidth: number;
    /** Unused depth when a board fits; zero otherwise. */
    readonly wastedDepth: number;
    /** `panelWidth × panelDepth / (maxWidth × maxDepth)` when a board fits. */
    readonly utilization: number;
}

/** Placement program report. */
export interface PlacementReport {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    readonly boardCount: number;
    readonly panelWidth: number;
    readonly panelDepth: number;
    readonly travelLength: number;
    readonly placementSeconds: number;
    /** Present and true when the program was produced with the 2-opt pass. */
    readonly optimized?: boolean;
    /** Millimetres saved by 2-opt; present when optimization was requested. */
    readonly travelSaved?: number;
    readonly entries: readonly PlacementEntry[];
}

/** One feeder copy on a balanced machine. */
export interface MachineFeeder {
    readonly id: string;
    readonly package: string;
    readonly assigned: number;
    readonly loaded: number;
    readonly ok: boolean;
}

/** One machine of a line balance. */
export interface MachineReport {
    readonly index: number;
    /** Reference designators in this machine's execution order. */
    readonly components: readonly string[];
    readonly travel: number;
    readonly seconds: number;
    readonly feeders: readonly MachineFeeder[];
}

/** Line balance report. */
export interface BalanceReport {
    readonly format: string;
    readonly version: number;
    readonly machines: readonly MachineReport[];
    readonly totalSeconds: number;
    readonly maxSeconds: number;
    readonly minSeconds: number;
    /** `(max − min) / max`, or zero when every machine takes zero seconds. */
    readonly imbalance: number;
    readonly ok: boolean;
}

/** Assignment strategy of a dual-lane schedule. */
export type DualLaneStrategy = 'greedy' | 'lpt';

/** One lane of a dual-lane schedule. */
export interface LaneSchedule {
    readonly index: number;
    /** Task reference designators in execution order. */
    readonly tasks: readonly string[];
    readonly seconds: number;
}

/** Dual-lane schedule report. */
export interface DualLaneReport {
    readonly format: string;
    readonly version: number;
    /** Assignment strategy that produced the report. */
    readonly strategy: DualLaneStrategy;
    readonly lanes: readonly LaneSchedule[];
    readonly singleLaneSeconds: number;
    readonly makespan: number;
    /** `makespan / singleLaneSeconds`, or zero when every task takes zero seconds. */
    readonly balanceRatio: number;
}

/** Multi-lane schedule report. */
export interface LanesReport {
    readonly format: string;
    readonly version: number;
    /** Assignment strategy that produced the report. */
    readonly strategy: DualLaneStrategy;
    /** Number of lanes the schedule uses, between 2 and 16. */
    readonly laneCount: number;
    readonly lanes: readonly LaneSchedule[];
    readonly singleLaneSeconds: number;
    readonly makespan: number;
    /** `makespan / singleLaneSeconds`, or zero when every task takes zero seconds. */
    readonly balanceRatio: number;
}

/** One evaluated reflow zone. */
export interface ZoneReport {
    readonly name: string;
    readonly length: number;
    readonly entryTemperature: number;
    readonly exitTemperature: number;
    readonly seconds: number;
    readonly secondsAboveLiquidus: number;
}

/** Evaluated reflow profile. */
export interface ReflowReport {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    readonly temperatureUnit: string;
    readonly conveyorSpeed: number;
    readonly liquidus: number;
    readonly peakTemperature: number;
    readonly timeAboveLiquidus: number;
    readonly totalSeconds: number;
    readonly zones: readonly ZoneReport[];
}

/** Full report returned by `report()`. */
export interface Report {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    readonly document: DocumentSummary;
    readonly validation: ValidationReport;
    readonly placement: PlacementReport;
    readonly feeders: FeederReport;
    readonly reflow: ReflowReport;
    readonly programLines: number;
}

/** Result of `exportDocument()`. */
export interface ExportReport {
    readonly format: ExportFormat;
    readonly path: string;
    readonly files: readonly string[];
    readonly bytes: number;
}

/** One aggregated entry of an import loss ledger. */
export interface ImportLoss {
    /** Stable code describing the skipped construct. */
    readonly code: string;
    /** Number of source constructs counted under `code`. */
    readonly count: number;
}

/** Result of `import()` or `importCsv()`. */
export interface ImportReport {
    /** Source format identifier: `kicad_pcb` or `csv`. */
    readonly format: string;
    /** Aggregated losses in code order. */
    readonly losses: readonly ImportLoss[];
    /** Number of components written to the document. */
    readonly componentCount: number;
    /** Board derived from the source outline. */
    readonly board: Board;
}

/** Session options. Constructing a session opens no file. */
export interface OpenSMTOptions {
    /** BCP-47 tag used to pick the CLI locale. Defaults to `en-US`. */
    readonly locale?: string;
}

/** Request of `setBoard()`. */
export interface BoardRequest {
    readonly width: number;
    readonly depth: number;
    readonly thickness: number;
}

/** Request of `setPanel()`; both gaps default to zero. */
export interface PanelRequest {
    readonly rows: number;
    readonly columns: number;
    readonly gapX?: number;
    readonly gapY?: number;
}

/** Request of `panelFit()`; both gaps default to the stored panel gaps. */
export interface PanelFitRequest {
    /** Maximum panel width in millimetres. */
    readonly maxWidth: number;
    /** Maximum panel depth in millimetres. */
    readonly maxDepth: number;
    /** Column gap override in millimetres. */
    readonly gapX?: number;
    /** Row gap override in millimetres. */
    readonly gapY?: number;
}

/** Request of `addComponent()`; `side` defaults to top and `rotation` to zero. */
export interface ComponentRequest {
    readonly refdes: string;
    readonly package: string;
    readonly side?: Side;
    readonly x: number;
    readonly y: number;
    readonly rotation?: number;
    readonly height?: number;
}

/** Request of `addFeeder()`; `loaded` defaults to `capacity`. */
export interface FeederRequest {
    readonly id: string;
    readonly package: string;
    readonly pitch: number;
    readonly capacity: number;
    readonly loaded?: number;
}

/** Request of `setProfile()`. */
export interface ReflowProfileRequest {
    readonly conveyorSpeed: number;
    readonly liquidus: number;
    readonly zones: readonly Zone[];
}

/** Disclosure level of one capability entry. */
export type SupportLevel = 'supported' | 'unsupported';

/** Engine capability matrix. */
export interface OpenSMTCapabilities {
    readonly engine: { readonly package: string; readonly version: string };
    readonly analysis: Readonly<Record<string, SupportLevel>>;
    readonly io: Readonly<Record<string, SupportLevel>>;
    readonly imports: Readonly<Record<string, SupportLevel>>;
    /** Caveats of supported capabilities, keyed by capability. */
    readonly limitations?: Readonly<Record<string, string>>;
}
