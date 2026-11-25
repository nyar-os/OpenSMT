#!/usr/bin/env node
/**
 * Command-line assembly only.
 *
 * `@wae/commander` owns argument parsing, help text and locale loading; every
 * command maps options onto one session operation of `OpenSMT`, so editing,
 * allocation, sequencing and reflow stay in the native core. The CLI keeps
 * printing and the exit-code contract (`capability/*` → 2, everything
 * else → 1).
 */

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { createCli, type CommandContext, type ParsedOptions } from '@wae/commander';

import type { SmtDocument } from './document.js';
import { NotImplementedError, OpenSMTError, type OpenSMTErrorCode } from './errors.js';
import { OpenSMT } from './open-smt.js';
import type { BalanceReport, DualLaneStrategy, ExportFormat, Side, Zone } from './types.js';

const here = path.dirname(fileURLToPath(import.meta.url));

const cli = createCli('open-smt');
cli.locales(path.join(here, '..', 'locales'));
cli.intro('cli.intro');

/** True once a command action has run, so framework parse failures can be told apart. */
let actionRan = false;

function failure(ctx: CommandContext, code: OpenSMTErrorCode, key: string, values?: Readonly<Record<string, string>>): OpenSMTError {
    return new OpenSMTError(code, ctx.t(`cli.error.${key}`, values as Record<string, string> | undefined));
}

function fileArg(options: ParsedOptions, ctx: CommandContext, usage: string): string {
    const value = options._[0];
    if (value === undefined) {
        throw failure(ctx, 'cli/missing-argument', 'missingArgument', { usage });
    }
    return value;
}

function optionText(options: ParsedOptions, name: string): string | undefined {
    const value = options[name];
    return typeof value === 'string' ? value : undefined;
}

function requiredOption(options: ParsedOptions, ctx: CommandContext, name: string): string {
    const value = optionText(options, name);
    if (value === undefined) {
        throw failure(ctx, 'cli/missing-option', 'missingOption', { option: `--${name}` });
    }
    return value;
}

function flag(options: ParsedOptions, name: string): boolean {
    return options[name] === true;
}

function numberOption(options: ParsedOptions, ctx: CommandContext, name: string): number | undefined {
    const raw = optionText(options, name);
    if (raw === undefined) {
        return undefined;
    }
    const value = Number(raw);
    if (!Number.isFinite(value)) {
        throw failure(ctx, 'cli/invalid-number', 'invalidNumber', { option: `--${name}`, value: raw });
    }
    return value;
}

function requiredNumber(options: ParsedOptions, ctx: CommandContext, name: string): number {
    const value = numberOption(options, ctx, name);
    if (value === undefined) {
        throw failure(ctx, 'cli/missing-option', 'missingOption', { option: `--${name}` });
    }
    return value;
}

function integerOption(options: ParsedOptions, ctx: CommandContext, name: string, fallback?: number): number {
    const value = numberOption(options, ctx, name) ?? fallback;
    if (value === undefined) {
        throw failure(ctx, 'cli/missing-option', 'missingOption', { option: `--${name}` });
    }
    if (!Number.isInteger(value) || value < 0) {
        throw failure(ctx, 'cli/invalid-number', 'invalidNumber', { option: `--${name}`, value: String(value) });
    }
    return value;
}

/** Parses `--boards` and rejects zero with a localized error before the addon runs. */
function boardCount(options: ParsedOptions, ctx: CommandContext): number {
    const boards = integerOption(options, ctx, 'boards');
    if (boards < 1) {
        throw failure(ctx, 'consumption/invalid', 'invalidBoards', { count: String(boards) });
    }
    return boards;
}

/** Parses `--boards` for the feeder schedule and rejects zero with `schedule/invalid`. */
function scheduleBoardCount(options: ParsedOptions, ctx: CommandContext): number {
    const boards = integerOption(options, ctx, 'boards');
    if (boards < 1) {
        throw failure(ctx, 'schedule/invalid', 'invalidScheduleBoards', { count: String(boards) });
    }
    return boards;
}

function exportFormat(value: string, ctx: CommandContext): ExportFormat {
    if (value === 'program' || value === 'csv') {
        return 'program';
    }
    if (value === 'feeders' || value === 'setup') {
        return 'feeders';
    }
    if (value === 'fai' || value === 'first-article') {
        return 'fai';
    }
    if (value === 'json' || value === 'md' || value === 'svg' || value === 'pdf') {
        return value;
    }
    if (value === 'stencil' || value === 'stencil-csv') {
        return 'stencil';
    }
    if (value === 'stencil-svg' || value === 'stencil-pdf') {
        return value;
    }
    if (value === 'kicad' || value === 'altium' || value === 'odb') {
        throw new NotImplementedError(`${value} export`);
    }
    throw failure(ctx, 'cli/unknown-format', 'unknownFormat', { format: value });
}

function dualStrategy(value: string, ctx: CommandContext): DualLaneStrategy {
    if (value === 'greedy' || value === 'lpt') {
        return value;
    }
    throw failure(ctx, 'schedule/invalid-strategy', 'invalidStrategy', { strategy: value });
}

function side(value: string): Side {
    if (value === 'top' || value === 'bottom') {
        return value;
    }
    throw new OpenSMTError('component/unknown-side', `unknown side \`${value}\`; only top or bottom is supported`);
}

/** Parses `--nozzles "0603=N1;QFN-32=N2"` into a package to nozzle map. */
function nozzleMap(raw: string | undefined, ctx: CommandContext): Record<string, string> {
    const nozzles = Object.create(null) as Record<string, string>;
    if (raw === undefined) {
        return nozzles;
    }
    for (const entry of raw.split(';')) {
        const trimmed = entry.trim();
        if (trimmed.length === 0) {
            continue;
        }
        const separator = trimmed.indexOf('=');
        if (separator <= 0 || separator === trimmed.length - 1) {
            throw failure(ctx, 'program/invalid', 'invalidNozzles', { entry: trimmed });
        }
        const packageName = trimmed.slice(0, separator).trim();
        const nozzle = trimmed.slice(separator + 1).trim();
        if (packageName.length === 0 || nozzle.length === 0) {
            throw failure(ctx, 'program/invalid', 'invalidNozzles', { entry: trimmed });
        }
        nozzles[packageName] = nozzle;
    }
    return nozzles;
}

function parseZones(raw: string): Zone[] {    return raw
        .split(';')
        .filter((entry) => entry.trim().length > 0)
        .map((entry) => {
            const parts = entry.split(':');
            if (parts.length !== 3) {
                throw new OpenSMTError('cli/invalid-number', `zone \`${entry}\` must be \`name:length:temperature\``);
            }
            const length = Number(parts[1]);
            const temperature = Number(parts[2]);
            if (!Number.isFinite(length) || !Number.isFinite(temperature)) {
                throw new OpenSMTError('cli/invalid-number', `zone \`${entry}\` has a non-numeric value`);
            }
            return { name: (parts[0] as string).trim(), length, temperature };
        });
}

function print(value: unknown): void {
    console.log(JSON.stringify(value, null, 2));
}

function round(value: number): string {
    return String(Number(value.toFixed(3)));
}

function printBalance(report: BalanceReport): void {
    const components = report.machines.reduce((sum, machine) => sum + machine.components.length, 0);
    const lines = [
        `Line balance: ${report.machines.length} machines, ${components} components`,
        `total ${round(report.totalSeconds)} s, max ${round(report.maxSeconds)} s, min ${round(report.minSeconds)} s, imbalance ${(report.imbalance * 100).toFixed(1)}%`,
    ];
    for (const machine of report.machines) {
        lines.push(
            `machine ${machine.index}: ${machine.components.length} components, ${round(machine.travel)} mm, ${round(machine.seconds)} s`,
        );
        if (machine.components.length > 0) {
            lines.push(`  ${machine.components.join(', ')}`);
        }
        if (machine.feeders.length > 0) {
            const feeders = machine.feeders
                .map((feeder) => `${feeder.id} ${feeder.assigned}/${feeder.loaded} ${feeder.ok ? 'ok' : 'short'}`)
                .join(', ');
            lines.push(`  feeders: ${feeders}`);
        }
    }
    lines.push(`coverage: ${report.ok ? 'ok' : 'short'}`);
    process.stdout.write(`${lines.join('\n')}\n`);
}

async function withSession<T>(work: (session: OpenSMT) => Promise<T>): Promise<T> {
    const session = new OpenSMT();
    try {
        return await work(session);
    } finally {
        await session.dispose();
    }
}

async function editDocument(
    options: ParsedOptions,
    ctx: CommandContext,
    usage: string,
    work: (document: SmtDocument) => Promise<void>,
): Promise<void> {
    const file = fileArg(options, ctx, usage);
    await withSession(async (session) => {
        const document = await session.open(file);
        await work(document);
    });
}

/** Runs one action and keeps the exit-code contract: `capability/*` → 2, everything else → 1. */
async function guard(ctx: CommandContext, work: () => Promise<void>): Promise<number> {
    actionRan = true;
    try {
        await work();
        return 0;
    } catch (error) {
        if (error instanceof OpenSMTError) {
            ctx.stderr(`${error.code}: ${error.message}`);
            return error.code.startsWith('capability/') ? 2 : 1;
        }
        ctx.stderr(error instanceof Error ? error.message : String(error));
        return 1;
    }
}

cli
    .command('create', 'cli.cmd.create')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            const file = fileArg(options, ctx, 'create <file>');
            await withSession(async (session) => {
                const document = await session.create(file);
                print(await document.save());
            });
        }),
    );

const importCommand = cli
    .command('import', 'cli.cmd.import')
    .option('--out <file>', 'cli.opt.out')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            const source = fileArg(options, ctx, 'import <file.kicad_pcb> --out <file.osmt>');
            await withSession(async (session) => {
                print(await session.import(source, requiredOption(options, ctx, 'out')));
            });
        }),
    );
importCommand
    .command('csv', 'cli.cmd.import-csv')
    .option('--out <file>', 'cli.opt.out')
    .option('--side <side>', 'cli.opt.side', { default: 'top' })
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            const source = fileArg(options, ctx, 'import csv <file.csv> --out <file.osmt> [--side top|bottom]');
            await withSession(async (session) => {
                print(await session.importCsv(source, requiredOption(options, ctx, 'out'), side(optionText(options, 'side') ?? 'top')));
            });
        }),
    );

cli
    .command('board', 'cli.cmd.board')
    .option('--width <mm>', 'cli.opt.width')
    .option('--depth <mm>', 'cli.opt.depth')
    .option('--thickness <mm>', 'cli.opt.thickness')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'board <file>', async (document) => {
                print(
                    await document.setBoard({
                        width: requiredNumber(options, ctx, 'width'),
                        depth: requiredNumber(options, ctx, 'depth'),
                        thickness: requiredNumber(options, ctx, 'thickness'),
                    }),
                );
            });
        }),
    );

const panelCommand = cli
    .command('panel', 'cli.cmd.panel')
    .option('--rows <n>', 'cli.opt.rows')
    .option('--columns <n>', 'cli.opt.columns')
    .option('--gap-x <mm>', 'cli.opt.gap-x')
    .option('--gap-y <mm>', 'cli.opt.gap-y')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'panel <file>', async (document) => {
                print(
                    await document.setPanel({
                        rows: integerOption(options, ctx, 'rows'),
                        columns: integerOption(options, ctx, 'columns'),
                        gapX: numberOption(options, ctx, 'gap-x') ?? 0,
                        gapY: numberOption(options, ctx, 'gap-y') ?? 0,
                    }),
                );
            });
        }),
    );
panelCommand
    .command('fit', 'cli.cmd.panel-fit')
    .option('--max-width <mm>', 'cli.opt.max-width')
    .option('--max-depth <mm>', 'cli.opt.max-depth')
    .option('--gap-x <mm>', 'cli.opt.gap-x')
    .option('--gap-y <mm>', 'cli.opt.gap-y')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'panel fit <file> --max-width <mm> --max-depth <mm> [--gap-x <mm>] [--gap-y <mm>]', async (document) => {
                print(
                    await document.panelFit({
                        maxWidth: requiredNumber(options, ctx, 'max-width'),
                        maxDepth: requiredNumber(options, ctx, 'max-depth'),
                        gapX: numberOption(options, ctx, 'gap-x'),
                        gapY: numberOption(options, ctx, 'gap-y'),
                    }),
                );
            });
        }),
    );
panelCommand
    .command('map', 'cli.cmd.panel-map')
    .option('--out <panel.json>', 'cli.opt.out')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'panel map <file> [--out <panel.json>]', async (document) => {
                const report = await document.panelMap();
                const target = optionText(options, 'out');
                if (target !== undefined) {
                    fs.writeFileSync(target, JSON.stringify(report, null, 2));
                }
                print(report);
            });
        }),
    );

const componentCommand = cli.command('component', 'cli.cmd.component');
componentCommand
    .command('add', 'cli.cmd.component-add')
    .option('--refdes <refdes>', 'cli.opt.refdes')
    .option('--package <pkg>', 'cli.opt.package')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .option('--side <side>', 'cli.opt.side', { default: 'top' })
    .option('--rotation <deg>', 'cli.opt.rotation', { default: '0' })
    .option('--height <mm>', 'cli.opt.height')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'component add <file>', async (document) => {
                print(
                    await document.addComponent({
                        refdes: requiredOption(options, ctx, 'refdes'),
                        package: requiredOption(options, ctx, 'package'),
                        side: side(optionText(options, 'side') ?? 'top'),
                        x: requiredNumber(options, ctx, 'x'),
                        y: requiredNumber(options, ctx, 'y'),
                        rotation: numberOption(options, ctx, 'rotation') ?? 0,
                        height: numberOption(options, ctx, 'height'),
                    }),
                );
            });
        }),
    );
componentCommand
    .command('update', 'cli.cmd.component-update')
    .option('--refdes <refdes>', 'cli.opt.refdes')
    .option('--package <pkg>', 'cli.opt.package')
    .option('--side <side>', 'cli.opt.side')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .option('--rotation <deg>', 'cli.opt.rotation')
    .option('--height <mm>', 'cli.opt.height')
    .option('--clear-height', 'cli.opt.clear-height')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'component update <file> --refdes <R1>', async (document) => {
                const configuredSide = optionText(options, 'side');
                print(
                    await document.updateComponent(requiredOption(options, ctx, 'refdes'), {
                        package: optionText(options, 'package'),
                        side: configuredSide === undefined ? undefined : side(configuredSide),
                        x: numberOption(options, ctx, 'x'),
                        y: numberOption(options, ctx, 'y'),
                        rotation: numberOption(options, ctx, 'rotation'),
                        height: numberOption(options, ctx, 'height'),
                        clearHeight: flag(options, 'clear-height'),
                    }),
                );
            });
        }),
    );
componentCommand
    .command('remove', 'cli.cmd.component-remove')
    .option('--refdes <refdes>', 'cli.opt.refdes')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'component remove <file>', async (document) => {
                print(await document.removeComponent(requiredOption(options, ctx, 'refdes')));
            });
        }),
    );
componentCommand
    .command('list', 'cli.cmd.component-list')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'component list <file>', async (document) => {
                print(await document.componentList());
            });
        }),
    );

const feederCommand = cli.command('feeder', 'cli.cmd.feeder');
feederCommand
    .command('add', 'cli.cmd.feeder-add')
    .option('--id <id>', 'cli.opt.id')
    .option('--package <pkg>', 'cli.opt.package')
    .option('--pitch <mm>', 'cli.opt.pitch')
    .option('--capacity <n>', 'cli.opt.capacity')
    .option('--loaded <n>', 'cli.opt.loaded')
    .option('--leader-waste <n>', 'cli.opt.leader-waste')
    .option('--trailer-waste <n>', 'cli.opt.trailer-waste')
    .option('--scrap <n>', 'cli.opt.scrap')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder add <file>', async (document) => {
                print(
                    await document.addFeeder({
                        id: requiredOption(options, ctx, 'id'),
                        package: requiredOption(options, ctx, 'package'),
                        pitch: requiredNumber(options, ctx, 'pitch'),
                        capacity: integerOption(options, ctx, 'capacity'),
                        loaded: numberOption(options, ctx, 'loaded'),
                        leaderWaste:
                            optionText(options, 'leader-waste') === undefined
                                ? undefined
                                : integerOption(options, ctx, 'leader-waste'),
                        trailerWaste:
                            optionText(options, 'trailer-waste') === undefined
                                ? undefined
                                : integerOption(options, ctx, 'trailer-waste'),
                        scrap: optionText(options, 'scrap') === undefined ? undefined : integerOption(options, ctx, 'scrap'),
                    }),
                );
            });
        }),
    );
feederCommand
    .command('update', 'cli.cmd.feeder-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--package <pkg>', 'cli.opt.package')
    .option('--pitch <mm>', 'cli.opt.pitch')
    .option('--capacity <n>', 'cli.opt.capacity')
    .option('--loaded <n>', 'cli.opt.loaded')
    .option('--leader-waste <n>', 'cli.opt.leader-waste')
    .option('--trailer-waste <n>', 'cli.opt.trailer-waste')
    .option('--scrap <n>', 'cli.opt.scrap')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder update <file> --id <id>', async (document) => {
                print(
                    await document.updateFeeder(requiredOption(options, ctx, 'id'), {
                        package: optionText(options, 'package'),
                        pitch: numberOption(options, ctx, 'pitch'),
                        capacity: optionText(options, 'capacity') === undefined ? undefined : integerOption(options, ctx, 'capacity'),
                        loaded: optionText(options, 'loaded') === undefined ? undefined : integerOption(options, ctx, 'loaded'),
                        leaderWaste: optionText(options, 'leader-waste') === undefined ? undefined : integerOption(options, ctx, 'leader-waste'),
                        trailerWaste: optionText(options, 'trailer-waste') === undefined ? undefined : integerOption(options, ctx, 'trailer-waste'),
                        scrap: optionText(options, 'scrap') === undefined ? undefined : integerOption(options, ctx, 'scrap'),
                    }),
                );
            });
        }),
    );
feederCommand
    .command('splice', 'cli.cmd.feeder-splice')
    .option('--id <id>', 'cli.opt.id')
    .option('--sequence <n>', 'cli.opt.sequence')
    .option('--leader-waste <n>', 'cli.opt.leader-waste')
    .option('--trailer-waste <n>', 'cli.opt.trailer-waste')
    .option('--scrap <n>', 'cli.opt.scrap')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder splice <file> --id <id> --sequence <n>', async (document) => {
                print(
                    await document.addSplice({
                        id: requiredOption(options, ctx, 'id'),
                        sequence: integerOption(options, ctx, 'sequence'),
                        leaderWaste:
                            optionText(options, 'leader-waste') === undefined
                                ? undefined
                                : integerOption(options, ctx, 'leader-waste'),
                        trailerWaste:
                            optionText(options, 'trailer-waste') === undefined
                                ? undefined
                                : integerOption(options, ctx, 'trailer-waste'),
                        scrap: optionText(options, 'scrap') === undefined ? undefined : integerOption(options, ctx, 'scrap'),
                    }),
                );
            });
        }),
    );
feederCommand
    .command('scrap', 'cli.cmd.feeder-scrap')
    .option('--id <id>', 'cli.opt.id')
    .option('--leader-waste <n>', 'cli.opt.leader-waste')
    .option('--trailer-waste <n>', 'cli.opt.trailer-waste')
    .option('--scrap <n>', 'cli.opt.scrap')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(
                options,
                ctx,
                'feeder scrap <file> --id <id> [--leader-waste <n>] [--trailer-waste <n>] [--scrap <n>]',
                async (document) => {
                    const leaderWaste =
                        optionText(options, 'leader-waste') === undefined ? undefined : integerOption(options, ctx, 'leader-waste');
                    const trailerWaste =
                        optionText(options, 'trailer-waste') === undefined ? undefined : integerOption(options, ctx, 'trailer-waste');
                    const scrap = optionText(options, 'scrap') === undefined ? undefined : integerOption(options, ctx, 'scrap');
                    if (leaderWaste === undefined && trailerWaste === undefined && scrap === undefined) {
                        throw failure(ctx, 'feeder/invalid', 'scrapRequired');
                    }
                    print(await document.updateFeeder(requiredOption(options, ctx, 'id'), { leaderWaste, trailerWaste, scrap }));
                },
            );
        }),
    );
feederCommand
    .command('remove', 'cli.cmd.feeder-remove')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder remove <file>', async (document) => {
                print(await document.removeFeeder(requiredOption(options, ctx, 'id')));
            });
        }),
    );
feederCommand
    .command('report', 'cli.cmd.feeder-report')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder report <file>', async (document) => {
                print(await document.feederReport());
            });
        }),
    );
feederCommand
    .command('optimize', 'cli.cmd.feeder-optimize')
    .option('--slot-pitch <mm>', 'cli.opt.slot-pitch')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder optimize <file> [--slot-pitch <mm>]', async (document) => {
                print(await document.feederOptimize(numberOption(options, ctx, 'slot-pitch')));
            });
        }),
    );
feederCommand
    .command('consumption', 'cli.cmd.feeder-consumption')
    .option('--boards <n>', 'cli.opt.boards')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder consumption <file> --boards <n>', async (document) => {
                print(await document.consumption(boardCount(options, ctx)));
            });
        }),
    );
feederCommand
    .command('schedule', 'cli.cmd.feeder-schedule')
    .option('--boards <n>', 'cli.opt.boards')
    .option('--out <file>', 'cli.opt.out')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeder schedule <file> --boards <n> [--out <file>]', async (document) => {
                const report = await document.schedule(scheduleBoardCount(options, ctx));
                const target = optionText(options, 'out');
                if (target !== undefined) {
                    fs.writeFileSync(target, JSON.stringify(report, null, 2));
                }
                print(report);
            });
        }),
    );

const bomCommand = cli.command('bom', 'cli.cmd.bom');
bomCommand
    .command('check', 'cli.cmd.bom-check')
    .option('--bom <bom.csv>', 'cli.opt.bom')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'bom check <file> --bom <bom.csv>', async (document) => {
                print(await document.bomCheck(requiredOption(options, ctx, 'bom')));
            });
        }),
    );

const qualityCommand = cli.command('quality', 'cli.cmd.quality');
qualityCommand
    .command('first-article', 'cli.cmd.quality-first-article')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'quality first-article <file>', async (document) => {
                print(await document.firstArticle());
            });
        }),
    );

const profileCommand = cli.command('profile', 'cli.cmd.profile');
profileCommand
    .command('set', 'cli.cmd.profile-set')
    .option('--speed <mm/min>', 'cli.opt.speed')
    .option('--liquidus <c>', 'cli.opt.liquidus')
    .option('--zones <zones>', 'cli.opt.zones')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'profile set <file>', async (document) => {
                print(
                    await document.setProfile({
                        conveyorSpeed: requiredNumber(options, ctx, 'speed'),
                        liquidus: requiredNumber(options, ctx, 'liquidus'),
                        zones: parseZones(requiredOption(options, ctx, 'zones')),
                    }),
                );
            });
        }),
    );

cli
    .command('validate', 'cli.cmd.validate')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'validate <file>', async (document) => {
                print(await document.validate());
            });
        }),
    );

cli
    .command('placement', 'cli.cmd.placement')
    .option('--out <program.json>', 'cli.opt.out')
    .option('--optimize', 'cli.opt.optimize')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'placement <file>', async (document) => {
                const report = await document.placement(flag(options, 'optimize'));
                const target = optionText(options, 'out');
                if (target !== undefined) {
                    fs.writeFileSync(target, JSON.stringify(report, null, 2));
                }
                print(report);
            });
        }),
    );

cli
    .command('stencil', 'cli.cmd.stencil')
    .option('--out <stencil.json>', 'cli.opt.out')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'stencil <file> [--out <stencil.json>]', async (document) => {
                const report = await document.stencil();
                const target = optionText(options, 'out');
                if (target !== undefined) {
                    fs.writeFileSync(target, JSON.stringify(report, null, 2));
                }
                print(report);
            });
        }),
    );

cli
    .command('clearance', 'cli.cmd.clearance')
    .option('--min-gap <mm>', 'cli.opt.min-gap')
    .option('--max-height <mm>', 'cli.opt.max-height')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(
                options,
                ctx,
                'clearance <file> [--min-gap <mm>] [--max-height <mm>]',
                async (document) => {
                    print(
                        await document.clearance({
                            minGap: numberOption(options, ctx, 'min-gap'),
                            maxHeight: numberOption(options, ctx, 'max-height'),
                        }),
                    );
                },
            );
        }),
    );

cli
    .command('feeders', 'cli.cmd.feeders')
    .option('--out <setup.csv>', 'cli.opt.out')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'feeders <file>', async (document) => {
                const report = await document.feederSetup();
                const target = optionText(options, 'out');
                if (target !== undefined) {
                    const rows = ['id,package,loaded,capacity,assigned,remaining,refdes'];
                    for (const feeder of report.feeders) {
                        rows.push(
                            [feeder.id, feeder.package, feeder.loaded, feeder.capacity, feeder.assigned, feeder.remaining, feeder.refdes.join('|')].join(','),
                        );
                    }
                    fs.writeFileSync(target, `${rows.join('\n')}\n`);
                }
                print(report);
            });
        }),
    );

cli
    .command('balance', 'cli.cmd.balance')
    .option('--machines <n>', 'cli.opt.machines')
    .option('--optimize', 'cli.opt.optimize')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'balance <file> --machines <n>', async (document) => {
                printBalance(await document.balance(integerOption(options, ctx, 'machines'), flag(options, 'optimize')));
            });
        }),
    );

const scheduleCommand = cli.command('schedule', 'cli.cmd.schedule');
scheduleCommand
    .command('dual', 'cli.cmd.schedule-dual')
    .option('--strategy <strategy>', 'cli.opt.strategy', { default: 'greedy' })
    .option('--improve', 'cli.opt.improve')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'schedule dual <file>', async (document) => {
                const strategy = dualStrategy(optionText(options, 'strategy') ?? 'greedy', ctx);
                print(await document.dualLane(strategy, flag(options, 'improve')));
            });
        }),
    );
scheduleCommand
    .command('lanes', 'cli.cmd.schedule-lanes')
    .option('--count <n>', 'cli.opt.count')
    .option('--strategy <strategy>', 'cli.opt.strategy', { default: 'greedy' })
    .option('--improve', 'cli.opt.improve')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'schedule lanes <file> --count <n>', async (document) => {
                const count = integerOption(options, ctx, 'count');
                if (count < 2 || count > 16) {
                    throw failure(ctx, 'schedule/invalid-lanes', 'invalidLanes', { count: String(count) });
                }
                const strategy = dualStrategy(optionText(options, 'strategy') ?? 'greedy', ctx);
                print(await document.lanes(count, strategy, flag(options, 'improve')));
            });
        }),
    );

const reflowCommand = cli
    .command('reflow', 'cli.cmd.reflow')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'reflow <file>', async (document) => {
                print(await document.reflow());
            });
        }),
    );
reflowCommand
    .command('window', 'cli.cmd.reflow-window')
    .option('--peak-min <c>', 'cli.opt.peak-min')
    .option('--peak-max <c>', 'cli.opt.peak-max')
    .option('--tal-min <s>', 'cli.opt.tal-min')
    .option('--tal-max <s>', 'cli.opt.tal-max')
    .option('--max-ramp <c/s>', 'cli.opt.max-ramp')
    .option('--max-cooling <c/s>', 'cli.opt.max-cooling')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(
                options,
                ctx,
                'reflow window <file> [--peak-min <c>] [--peak-max <c>] [--tal-min <s>] [--tal-max <s>] [--max-ramp <c/s>] [--max-cooling <c/s>]',
                async (document) => {
                    print(
                        await document.reflowWindow({
                            peakMin: numberOption(options, ctx, 'peak-min'),
                            peakMax: numberOption(options, ctx, 'peak-max'),
                            timeAboveLiquidusMin: numberOption(options, ctx, 'tal-min'),
                            timeAboveLiquidusMax: numberOption(options, ctx, 'tal-max'),
                            maxHeatingRate: numberOption(options, ctx, 'max-ramp'),
                            maxCoolingRate: numberOption(options, ctx, 'max-cooling'),
                        }),
                    );
                },
            );
        }),
    );

const taktCommand = cli
    .command('takt', 'cli.cmd.takt')
    .option('--place-seconds <s>', 'cli.opt.place-seconds')
    .option('--head-speed <mm/s>', 'cli.opt.head-speed')
    .option('--feeder-change-seconds <s>', 'cli.opt.feeder-change-seconds')
    .option('--feeder-changes <n>', 'cli.opt.feeder-changes', { default: '0' })
    .option('--nozzle-change-seconds <s>', 'cli.opt.nozzle-change-seconds')
    .option('--nozzle-changes <n>', 'cli.opt.nozzle-changes', { default: '0' })
    .option('--machines <n>', 'cli.opt.machines', { default: '1' })
    .option('--optimize', 'cli.opt.optimize')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(
                options,
                ctx,
                'takt <file> [--place-seconds <s>] [--head-speed <mm/s>] [--feeder-change-seconds <s>] [--feeder-changes <n>] [--nozzle-change-seconds <s>] [--nozzle-changes <n>] [--machines <n>] [--optimize]',
                async (document) => {
                    print(
                        await document.takt({
                            placeSeconds: numberOption(options, ctx, 'place-seconds'),
                            headSpeed: numberOption(options, ctx, 'head-speed'),
                            feederChangeSeconds: numberOption(options, ctx, 'feeder-change-seconds'),
                            feederChanges: integerOption(options, ctx, 'feeder-changes', 0),
                            nozzleChangeSeconds: numberOption(options, ctx, 'nozzle-change-seconds'),
                            nozzleChanges: integerOption(options, ctx, 'nozzle-changes', 0),
                            machineCount: integerOption(options, ctx, 'machines', 1),
                            optimize: flag(options, 'optimize'),
                        }),
                    );
                },
            );
        }),
    );
taktCommand
    .command('heatmap', 'cli.cmd.takt-heatmap')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'takt heatmap <file>', async (document) => {
                print(await document.taktHeatmap());
            });
        }),
    );

const machineCommand = cli.command('machine', 'cli.cmd.machine');
machineCommand
    .command('export', 'cli.cmd.machine-export')
    .option('--out <file>', 'cli.opt.out')
    .option('--nozzles <map>', 'cli.opt.nozzles')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'machine export <file> --out <file.csv> [--nozzles "pkg=nozzle;..."]', async (document) => {
                print(
                    await document.machineExport(
                        requiredOption(options, ctx, 'out'),
                        nozzleMap(optionText(options, 'nozzles'), ctx),
                    ),
                );
            });
        }),
    );
machineCommand
    .command('verify', 'cli.cmd.machine-verify')
    .option('--program <file>', 'cli.opt.program')
    .option('--nozzles <map>', 'cli.opt.nozzles')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'machine verify <file> --program <file.csv> [--nozzles "pkg=nozzle;..."]', async (document) => {
                print(
                    await document.machineVerify(
                        requiredOption(options, ctx, 'program'),
                        nozzleMap(optionText(options, 'nozzles'), ctx),
                    ),
                );
            });
        }),
    );

cli
    .command('changeover', 'cli.cmd.changeover')
    .option('--feeder-change-seconds <s>', 'cli.opt.feeder-change-seconds')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'changeover <current.osmt> <next.osmt> [--feeder-change-seconds <s>]', async (document) => {
                const next = options._[1];
                if (next === undefined) {
                    throw failure(ctx, 'cli/missing-argument', 'missingArgument', {
                        usage: 'changeover <current.osmt> <next.osmt> [--feeder-change-seconds <s>]',
                    });
                }
                print(await document.changeover(next, numberOption(options, ctx, 'feeder-change-seconds')));
            });
        }),
    );

cli
    .command('export', 'cli.cmd.export')
    .option('--format <format>', 'cli.opt.format', { default: 'program' })
    .option('--out <path>', 'cli.opt.out')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'export <file> --format <format> --out <path>', async (document) => {
                const format = exportFormat(optionText(options, 'format') ?? 'program', ctx);
                print(await document.exportDocument(format, requiredOption(options, ctx, 'out')));
            });
        }),
    );

cli
    .command('report', 'cli.cmd.report')
    .actionCtx((options, _args, ctx) =>
        guard(ctx, async () => {
            await editDocument(options, ctx, 'report <file>', async (document) => {
                print(await document.report());
            });
        }),
    );

cli
    .command('capabilities', 'cli.cmd.capabilities')
    .actionCtx((_options, _args, ctx) =>
        guard(ctx, async () => {
            await withSession(async (session) => {
                print(await session.capabilities());
            });
        }),
    );

cli
    .command('version', 'cli.cmd.version')
    .actionCtx((_options, _args, ctx) =>
        guard(ctx, async () => {
            console.log('0.0.0');
        }),
    );

const entry = process.argv[1];
if (entry !== undefined && import.meta.url === pathToFileURL(entry).href) {
    try {
        const code = await cli.parse(process.argv);
        // Framework parse/validation failures return 2 before any action ran;
        // the CLI contract reserves 2 for `capability/*` only.
        process.exitCode = code === 2 && !actionRan ? 1 : code;
    } catch (error) {
        process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
        process.exitCode = 1;
    }
}
