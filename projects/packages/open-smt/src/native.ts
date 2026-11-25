/**
 * Native addon discovery.
 *
 * The addon only maps values: document I/O, allocation, sequencing and reflow
 * live in the shared Rust `smt-model` crate. An absent addon is a declarative
 * state, so calls report `capability/not-implemented` instead of inventing a
 * result.
 */

import { createRequire } from 'node:module';

const requireNative = createRequire(import.meta.url);

/** Orthogonal exports of the built addon. Every call returns a JSON string. */
export interface NativeBinding {
    version(): string;
    capabilities(): string;
    create(path: string): string;
    open(path: string): string;
    import(source: string, target: string): string;
    import_csv(source: string, target: string, side?: string): string;
    save(path: string, target?: string): string;
    board_set(path: string, requestJson: string): string;
    panel_set(path: string, requestJson: string): string;
    component_add(path: string, requestJson: string): string;
    component_update(path: string, requestJson: string): string;
    component_remove(path: string, requestJson: string): string;
    feeder_add(path: string, requestJson: string): string;
    feeder_update(path: string, requestJson: string): string;
    feeder_splice(path: string, requestJson: string): string;
    feeder_remove(path: string, requestJson: string): string;
    profile_set(path: string, requestJson: string): string;
    validate(path: string): string;
    placement(path: string, optimize?: boolean): string;
    feeders(path: string): string;
    component_list(path: string): string;
    feeder_report(path: string): string;
    panel_map(path: string): string;
    stencil(path: string): string;
    paste_volume(path: string, requestJson?: string): string;
    paste_csv(path: string, target: string, requestJson?: string): string;
    clearance(path: string, requestJson?: string): string;
    first_article(path: string): string;
    panel_fit(path: string, requestJson: string): string;
    bom_check(path: string, bomPath: string): string;
    balance(path: string, machines: number, optimize?: boolean): string;
    dual_lane(path: string, strategy?: string, improve?: boolean): string;
    schedule_lanes(path: string, lanes: number, strategy?: string, improve?: boolean): string;
    reflow(path: string): string;
    reflow_window(path: string, requestJson?: string): string;
    consumption(path: string, requestJson: string): string;
    feeder_schedule(path: string, requestJson: string): string;
    takt(path: string, requestJson?: string): string;
    takt_heatmap(path: string): string;
    machine_export(path: string, target: string, requestJson?: string): string;
    machine_verify(path: string, source: string, requestJson?: string): string;
    feeder_optimize(path: string, requestJson?: string): string;
    changeover(current: string, next: string, requestJson?: string): string;
    export(path: string, format: string, target: string): string;
    report(path: string): string;
}

let cached: NativeBinding | null | undefined;

/** Loads the addon once, or returns `undefined` when it is not installed. */
export function nativeBinding(): NativeBinding | undefined {
    if (cached === undefined) {
        cached = load();
    }
    return cached ?? undefined;
}

function load(): NativeBinding | null {
    for (const candidate of candidates()) {
        try {
            return requireNative(candidate) as NativeBinding;
        } catch (error) {
            if (candidate === explicitPath()) {
                throw error;
            }
            if (!isModuleNotFound(error, candidate)) {
                throw error;
            }
        }
    }
    return null;
}

function candidates(): string[] {
    const explicit = explicitPath();
    if (explicit) {
        return [explicit];
    }
    return [`@wae/open-smt-${process.platform}-${process.arch}`];
}

function explicitPath(): string | undefined {
    const value = process.env.OPEN_SMT_NATIVE;
    return value !== undefined && value.length > 0 ? value : undefined;
}

function isModuleNotFound(error: unknown, candidate: string): boolean {
    if (typeof error !== 'object' || error === null) {
        return false;
    }
    const code = (error as { code?: unknown }).code;
    const message = (error as Error).message.replace(/\\/g, '/');
    return code === 'MODULE_NOT_FOUND' && message.includes(candidate);
}
