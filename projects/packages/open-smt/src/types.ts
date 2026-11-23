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
export type ExportFormat = 'program' | 'feeders' | 'fai' | 'json' | 'md';

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

/** One board instance of the panel array. */
export interface PanelInstance {
    /** Board number, starting at 1 in row-major order. */
    readonly board: number;
    /** Panel row, starting at 1. */
    readonly row: number;
    /** Panel column, starting at 1. */
    readonly column: number;
    /** X offset of the board origin, `(column - 1) × (width + gapX)`. */
    readonly offsetX: number;
    /** Y offset of the board origin, `(row - 1) × (depth + gapY)`. */
    readonly offsetY: number;
}

/** One component placement in panel coordinates. */
export interface PanelPlacement {
    /** Board number the placement belongs to, starting at 1. */
    readonly board: number;
    /** Panel X coordinate, `offsetX + component x`. */
    readonly x: number;
    /** Panel Y coordinate, `offsetY + component y`. */
    readonly y: number;
}

/** One component with its placement on every board instance. */
export interface MappedComponent {
    readonly refdes: string;
    readonly package: string;
    readonly side: Side;
    /** X coordinate on the single board. */
    readonly x: number;
    /** Y coordinate on the single board. */
    readonly y: number;
    readonly rotation: number;
    /** Placements in board order; empty when the coordinates are not finite. */
    readonly instances: readonly PanelPlacement[];
}

/** Read-only single-board to panel coordinate map. */
export interface PanelMapReport {
    readonly format: 'open-smt-panel-map';
    readonly version: number;
    readonly units: string;
    readonly width: number;
    readonly depth: number;
    readonly thickness: number;
    readonly rows: number;
    readonly columns: number;
    readonly gapX: number;
    readonly gapY: number;
    readonly boardCount: number;
    readonly panelWidth: number;
    readonly panelDepth: number;
    /** Board origins in row-major order, one per board. */
    readonly instances: readonly PanelInstance[];
    /** Components in document order, each with its board placements. */
    readonly components: readonly MappedComponent[];
    /** Components whose coordinates fall outside the single board. */
    readonly outsideBoard: number;
    /** Whether every component lies inside the single board. */
    readonly ok: boolean;
}

/** One reference designator the BOM and the placement table disagree about. */
export interface BomFinding {
    readonly package: string;
    /** The disputed reference designator. */
    readonly refdes: string;
}

/** One package whose placed count differs from the BOM quantity. */
export interface BomQuantityMismatch {
    readonly package: string;
    /** Quantity the BOM declares for this package. */
    readonly bomQuantity: number;
    /** Number of placements of this package. */
    readonly placedQuantity: number;
    /** `placedQuantity - bomQuantity`. */
    readonly difference: number;
}

/** Read-only BOM cross-check report. */
export interface BomCheckReport {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    /** Whether the placement table matches the BOM. */
    readonly ok: boolean;
    /** Number of data rows in the BOM. */
    readonly bomLines: number;
    /** Number of placements in the document. */
    readonly components: number;
    /** BOM reference designators with no placement, in package then refdes order. */
    readonly missing: readonly BomFinding[];
    /** Placed reference designators with no BOM entry, in package then refdes order. */
    readonly extra: readonly BomFinding[];
    /** Packages whose counts disagree, in package order. */
    readonly quantityMismatches: readonly BomQuantityMismatch[];
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

/**
 * Machine model of a cycle time estimate.
 *
 * Omitted fields keep their defaults: `0.6` s per component, `200` mm/s head
 * speed, `30` s per feeder change, `5` s per nozzle change, zero changes per
 * board, one machine and no 2-opt pass.
 */
export interface TaktOptions {
    /** Fixed pick-and-place overhead per component, in seconds. */
    readonly placeSeconds?: number;
    /** Head travel speed in millimetres per second. */
    readonly headSpeed?: number;
    /** Time to change one feeder reel, in seconds. */
    readonly feederChangeSeconds?: number;
    /** Feeder changes assumed per board. */
    readonly feederChanges?: number;
    /** Time to switch one nozzle, in seconds. */
    readonly nozzleChangeSeconds?: number;
    /** Nozzle changes assumed per board. */
    readonly nozzleChanges?: number;
    /** Number of machines the work is split across evenly, 1 to 64. */
    readonly machineCount?: number;
    /** Whether the sequence is improved with the 2-opt pass first. */
    readonly optimize?: boolean;
}

/** Read-only cycle time and throughput estimate of one board. */
export interface TaktReport {
    readonly format: string;
    readonly version: number;
    readonly units: string;
    /** Number of placement points on one board. */
    readonly components: number;
    /** Nozzle pick-and-place cycles per board; one per component. */
    readonly picks: number;
    /** Head travel of the program, in millimetres, matching the sequence. */
    readonly travelMm: number;
    /** Placement segment of the estimate, in seconds. */
    readonly placeSeconds: number;
    /** Travel segment of the estimate, in seconds. */
    readonly travelSeconds: number;
    /** Feeder and nozzle change overhead, in seconds. */
    readonly overheadSeconds: number;
    /** Estimated time of one board on one machine, in seconds. */
    readonly boardSeconds: number;
    /** Number of machines the work is split across. */
    readonly machineCount: number;
    /** `boardSeconds / machineCount`, the effective per-machine board time. */
    readonly machineSeconds: number;
    /** Boards produced per hour; zero when a machine takes zero seconds. */
    readonly boardsPerHour: number;
    /** Components placed per hour across every machine. */
    readonly componentsPerHour: number;
    /** Present and true when the sequence was produced with the 2-opt pass. */
    readonly optimized?: boolean;
    /** Millimetres saved by 2-opt; present when optimization was requested. */
    readonly travelSaved?: number;
}

/** One feeder lane placed at one tape-bank slot. */
export interface StationAssignment {
    /** Slot number, starting at 1 in assignment order. */
    readonly slot: number;
    /** Slot position along the tape bank, `(slot - 1) × slotPitch`. */
    readonly positionMm: number;
    /** Feeder lane sitting at this slot. */
    readonly feederId: string;
    /** Package supplied by the lane. */
    readonly package: string;
    /** Picks the lane supplies in the placement program. */
    readonly picks: number;
    /** Mean placement X of the picks; `null` for a lane the program never uses. */
    readonly meanX: number | null;
}

/** Read-only feeder station assignment report. */
export interface StationReport {
    readonly format: 'open-smt-station';
    readonly version: number;
    readonly units: string;
    /** Slot pitch used for the positions, in millimetres. */
    readonly slotPitch: number;
    /** Number of assigned slots; one per feeder lane. */
    readonly slotCount: number;
    /** Tape-bank travel of the stored feeder order, in millimetres. */
    readonly baselineTravelMm: number;
    /** Tape-bank travel of the optimized assignment, in millimetres. */
    readonly travelMm: number;
    /** `baselineTravelMm - travelMm`, floored at zero. */
    readonly travelSaved: number;
    /** One entry per lane, in slot order. */
    readonly stations: readonly StationAssignment[];
}

/** What one changeover step does to a feeder lane. */
export type ChangeoverKind = 'remove' | 'swap' | 'add';

/** One step of the changeover sequence. */
export interface ChangeoverAction {
    /** One-based position in the sequence. */
    readonly step: number;
    readonly kind: ChangeoverKind;
    /** Feeder lane the action touches. */
    readonly id: string;
    /** Package of the removed lane; present only for `remove`. */
    readonly package?: string;
    /** Package before the swap; present only for `swap`. */
    readonly fromPackage?: string;
    /** Package after the swap; present only for `swap`. */
    readonly toPackage?: string;
    /** Tape pitch before the swap; present only for `swap`. */
    readonly fromPitch?: number;
    /** Tape pitch after the swap; present only for `swap`. */
    readonly toPitch?: number;
    /** Lane capacity to load; present only for `add`. */
    readonly capacity?: number;
    /** Loaded quantity to load; present only for `add`. */
    readonly loaded?: number;
}

/** Read-only feeder changeover sequence report. */
export interface ChangeoverReport {
    readonly format: 'open-smt-changeover';
    readonly version: number;
    /** Number of lanes in the current document. */
    readonly currentFeeders: number;
    /** Number of lanes in the next document. */
    readonly nextFeeders: number;
    /** Actions in execution order: removals, swaps, additions. */
    readonly actions: readonly ChangeoverAction[];
    /** Lane ids that match by package and exact pitch, in ascending id order. */
    readonly unchanged: readonly string[];
    /** Number of physical reel changes; a swap counts twice. */
    readonly changes: number;
    /** Time assumed for one reel change, in seconds. */
    readonly feederChangeSeconds: number;
    /** `changes × feederChangeSeconds`. */
    readonly estimatedSeconds: number;
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

/**
 * Limits of a reflow process-window check.
 *
 * Every limit is optional, so a request names only the metrics it cares about;
 * an omitted limit produces no check for that metric.
 */
export interface ReflowWindowLimits {
    /** Lowest acceptable peak temperature, in degrees Celsius. */
    readonly peakMin?: number;
    /** Highest acceptable peak temperature, in degrees Celsius. */
    readonly peakMax?: number;
    /** Shortest acceptable time above liquidus, in seconds. */
    readonly timeAboveLiquidusMin?: number;
    /** Longest acceptable time above liquidus, in seconds. */
    readonly timeAboveLiquidusMax?: number;
    /** Highest acceptable heating rate, in degrees Celsius per second. */
    readonly maxHeatingRate?: number;
    /** Highest acceptable cooling rate, in degrees Celsius per second. */
    readonly maxCoolingRate?: number;
}

/** One zone with its signed ramp rate. */
export interface ReflowWindowZone {
    readonly name: string;
    readonly seconds: number;
    readonly entryTemperature: number;
    readonly exitTemperature: number;
    /** Signed rate `(exit − entry) / seconds`; negative means cooling. */
    readonly rampRate: number;
}

/** One metric compared with one limit. */
export interface ReflowWindowCheck {
    /** Metric compared. */
    readonly metric: 'peakTemperature' | 'timeAboveLiquidus' | 'maxHeatingRate' | 'maxCoolingRate';
    /** Direction of the limit. */
    readonly kind: 'min' | 'max';
    /** Profile value of the metric. */
    readonly value: number;
    /** Limit the value was compared with. */
    readonly limit: number;
    /** `value − limit` for a minimum, `limit − value` for a maximum. */
    readonly margin: number;
    /** Whether the margin is not negative. */
    readonly pass: boolean;
}

/** Read-only reflow process-window report. */
export interface ReflowWindowReport {
    readonly format: 'open-smt-reflow-window';
    readonly version: number;
    readonly units: string;
    readonly temperatureUnit: string;
    readonly conveyorSpeed: number;
    readonly liquidus: number;
    readonly peakTemperature: number;
    readonly timeAboveLiquidus: number;
    readonly totalSeconds: number;
    /** Highest positive zone ramp rate, or zero when no zone heats. */
    readonly maxHeatingRate: number;
    /** Highest cooling rate magnitude, or zero when no zone cools. */
    readonly maxCoolingRate: number;
    readonly zones: readonly ReflowWindowZone[];
    /** One entry per supplied limit, in a fixed metric order. */
    readonly checks: readonly ReflowWindowCheck[];
    /** Whether every supplied limit is met; true when no limit was supplied. */
    readonly ok: boolean;
}

/** Consumption of one feeder lane over a run. */
export interface FeederConsumption {
    readonly id: string;
    readonly package: string;
    /** Components the lane supplies on one board. */
    readonly picksPerBoard: number;
    /** Parts the lane consumes over the whole run. */
    readonly picks: number;
    /** Maximum parts on one reel of this lane. */
    readonly capacity: number;
    /** Parts currently loaded on the lane. */
    readonly loaded: number;
    /** Full reels needed for the run; zero when the lane supplies nothing. */
    readonly reels: number;
    /** Parts on the last reel, between 1 and `capacity`; zero when no reel is needed. */
    readonly remainder: number;
    /** Whether the currently loaded quantity covers the run. */
    readonly short: boolean;
}

/** Consumption of one package over a run. */
export interface PackageConsumption {
    readonly package: string;
    /** Components of this package on one board. */
    readonly componentsPerBoard: number;
    /** Parts of this package consumed over the run. */
    readonly totalPicks: number;
    /** Lanes that supply this package. */
    readonly feeders: number;
    /** Reels needed across every lane of this package. */
    readonly reels: number;
    /** Whether any lane of this package is short for the run. */
    readonly short: boolean;
}

/** Read-only feeder consumption plan for a build quantity. */
export interface ConsumptionReport {
    readonly format: 'open-smt-consumption';
    readonly version: number;
    readonly units: string;
    /** Boards the plan covers. */
    readonly boards: number;
    /** Components on one board. */
    readonly componentsPerBoard: number;
    /** Parts the run consumes across every lane. */
    readonly totalPicks: number;
    readonly packages: readonly PackageConsumption[];
    readonly feeders: readonly FeederConsumption[];
    /** Whether the loaded quantities cover the run. */
    readonly ok: boolean;
}

/** Generic checks a first-article sheet tells an inspector to perform. */
export type FirstArticleCheck = 'placement' | 'orientation' | 'package' | 'feeder';

/** One lane that supplies a package on the first-article sheet. */
export interface FirstArticleLane {
    readonly id: string;
    /** Tape pitch in millimetres. */
    readonly pitch: number;
    readonly loaded: number;
    readonly capacity: number;
    /** Components the allocation assigns to the lane. */
    readonly assigned: number;
}

/** Material block of one package on the first-article sheet. */
export interface FirstArticleMaterial {
    readonly package: string;
    /** Components of this package on one board. */
    readonly components: number;
    /** Distinct board sides, `top` before `bottom`. */
    readonly sides: readonly Side[];
    /** Lanes that supply the package, in id order. */
    readonly lanes: readonly FirstArticleLane[];
}

/** One row of the first-article inspection sheet. */
export interface FirstArticleStep {
    /** One-based row number in program order. */
    readonly step: number;
    readonly refdes: string;
    readonly package: string;
    readonly side: Side;
    readonly x: number;
    readonly y: number;
    readonly rotation: number;
    /** Feeder lane that supplies the component. */
    readonly feeder: string;
    /** Tape pitch of that lane, in millimetres. */
    readonly pitch: number;
}

/** Read-only first-article inspection sheet. */
export interface FirstArticleReport {
    readonly format: 'open-smt-first-article';
    readonly version: number;
    readonly units: string;
    /** Document revision the sheet was produced from. */
    readonly revision: number;
    readonly board: Board;
    readonly panel: Panel;
    readonly boardCount: number;
    readonly panelWidth: number;
    readonly panelDepth: number;
    /** Components on one board. */
    readonly components: number;
    readonly topComponents: number;
    readonly bottomComponents: number;
    /** Distinct packages. */
    readonly packages: number;
    /** Declared feeder lanes. */
    readonly feeders: number;
    /** Generic checks the sheet supports, in fixed order. */
    readonly checklist: readonly FirstArticleCheck[];
    /** One material block per package, in package order. */
    readonly materials: readonly FirstArticleMaterial[];
    /** One inspection row per placement step, in program order. */
    readonly steps: readonly FirstArticleStep[];
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

/**
 * Request of `updateComponent()`; the refdes identifies the component.
 *
 * Every supplied field replaces the stored value and omitted fields keep
 * theirs. `rotation` is normalised to `[0, 360)` when supplied. `clearHeight`
 * sets the height back to `null` and conflicts with `height`.
 */
export interface ComponentUpdateRequest {
    readonly package?: string;
    readonly side?: Side;
    readonly x?: number;
    readonly y?: number;
    readonly rotation?: number;
    readonly height?: number;
    readonly clearHeight?: boolean;
}

/** Request of `addFeeder()`; `loaded` defaults to `capacity`. */
export interface FeederRequest {
    readonly id: string;
    readonly package: string;
    readonly pitch: number;
    readonly capacity: number;
    readonly loaded?: number;
}

/**
 * Request of `updateFeeder()`; the id identifies the feeder.
 *
 * Every supplied field replaces the stored value and omitted fields keep
 * theirs, so at least one field must be supplied.
 */
export interface FeederUpdateRequest {
    readonly package?: string;
    readonly pitch?: number;
    readonly capacity?: number;
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
