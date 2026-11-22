#!/usr/bin/env node
/**
 * Command-line assembly only.
 *
 * Every command maps options onto one session operation of `OpenSMT`; editing,
 * allocation, sequencing and reflow stay in the native core. The CLI owns
 * argument parsing, help text, printing and exit codes, and nothing else.
 */

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import type { SmtDocument } from './document.js';
import { NotImplementedError, OpenSMTError, type OpenSMTErrorCode } from './errors.js';
import { OpenSMT } from './open-smt.js';
import type { BalanceReport, ExportFormat, Side, Zone } from './types.js';

interface CliLocale {
    readonly cli: {
        readonly intro: string;
        readonly usage: readonly string[];
        readonly exit: string;
        readonly message: Readonly<Record<string, string>>;
        readonly error: Readonly<Record<string, string>>;
    };
}

const here = path.dirname(fileURLToPath(import.meta.url));
const locale = JSON.parse(
    fs.readFileSync(path.join(here, '..', 'locales', 'en-US', 'cli.json'), 'utf8'),
) as CliLocale;

function fill(template: string, values: Readonly<Record<string, string>> = {}): string {
    return template.replace(/\{(\w+)\}/g, (_match, key: string) => values[key] ?? `{${key}}`);
}

function failure(code: OpenSMTErrorCode, key: string, values?: Readonly<Record<string, string>>): OpenSMTError {
    return new OpenSMTError(code, fill(locale.cli.error[key] ?? key, values));
}

function printUsage(): void {
    process.stdout.write(`${locale.cli.intro}\n\n${locale.cli.usage.join('\n')}\n\n${locale.cli.exit}\n`);
}

interface Parsed {
    readonly positionals: readonly string[];
    readonly options: ReadonlyMap<string, string>;
    readonly flags: ReadonlySet<string>;
}

function parse(args: readonly string[], valueOptions: readonly string[], flagOptions: readonly string[]): Parsed {
    const positionals: string[] = [];
    const flags = new Set<string>();
    const options = new Map<string, string>();
    let index = 0;
    while (index < args.length) {
        const argument = args[index] as string;
        if (argument.startsWith('--')) {
            if (flagOptions.includes(argument)) {
                flags.add(argument);
                index += 1;
            } else if (valueOptions.includes(argument)) {
                const value = args[index + 1];
                if (value === undefined || value.startsWith('--')) {
                    throw failure('cli/missing-value', 'missingValue', { option: argument });
                }
                options.set(argument, value);
                index += 2;
            } else {
                throw failure('cli/unknown-option', 'unknownOption', { option: argument });
            }
        } else {
            positionals.push(argument);
            index += 1;
        }
    }
    return { positionals, options, flags };
}

function positional(parsed: Parsed, index: number, usage: string): string {
    const value = parsed.positionals[index];
    if (value === undefined) {
        throw failure('cli/missing-argument', 'missingArgument', { usage });
    }
    return value;
}

function required(parsed: Parsed, name: string): string {
    const value = parsed.options.get(name);
    if (value === undefined) {
        throw failure('cli/missing-option', 'missingOption', { option: name });
    }
    return value;
}

function number(parsed: Parsed, name: string): number | undefined {
    const raw = parsed.options.get(name);
    if (raw === undefined) {
        return undefined;
    }
    const value = Number(raw);
    if (!Number.isFinite(value)) {
        throw failure('cli/invalid-number', 'invalidNumber', { option: name, value: raw });
    }
    return value;
}

function requiredNumber(parsed: Parsed, name: string): number {
    const value = number(parsed, name);
    if (value === undefined) {
        throw failure('cli/missing-option', 'missingOption', { option: name });
    }
    return value;
}

function integer(parsed: Parsed, name: string, fallback?: number): number {
    const value = number(parsed, name) ?? fallback;
    if (value === undefined) {
        throw failure('cli/missing-option', 'missingOption', { option: name });
    }
    if (!Number.isInteger(value) || value < 0) {
        throw failure('cli/invalid-number', 'invalidNumber', { option: name, value: String(value) });
    }
    return value;
}

function exportFormat(value: string): ExportFormat {
    if (value === 'program' || value === 'csv') {
        return 'program';
    }
    if (value === 'feeders' || value === 'setup') {
        return 'feeders';
    }
    if (value === 'json' || value === 'md') {
        return value;
    }
    if (value === 'kicad' || value === 'altium' || value === 'odb') {
        throw new NotImplementedError(`${value} export`);
    }
    throw failure('cli/unknown-format', 'unknownFormat', { format: value });
}

function side(value: string): Side {
    if (value === 'top' || value === 'bottom') {
        return value;
    }
    throw new OpenSMTError('component/unknown-side', `unknown side \`${value}\`; only top or bottom is supported`);
}

function parseZones(raw: string): Zone[] {
    return raw
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

async function dispatch(args: readonly string[]): Promise<number> {
    const command = args[0];
    if (command === undefined) {
        printUsage();
        return 0;
    }
    switch (command) {
        case '-h':
        case '--help':
        case 'help':
            printUsage();
            return 0;
        case 'version':
            console.log('0.0.0');
            return 0;
        case 'capabilities':
            return withSession(async (session) => {
                print(await session.capabilities());
                return 0;
            });
        case 'create':
            return withSession(async (session) => {
                const file = positional(parse(args.slice(1), [], []), 0, 'create <file>');
                const document = await session.create(file);
                print(await document.save());
                return 0;
            });
        case 'import':
            return withSession(async (session) => {
                const parsed = parse(args.slice(1), ['--out'], []);
                const source = positional(parsed, 0, 'import <file.kicad_pcb> --out <file.osmt>');
                print(await session.import(source, required(parsed, '--out')));
                return 0;
            });
        case 'board':
            return editDocument(args.slice(1), 'board <file>', ['--width', '--depth', '--thickness'], async (document, parsed) => {
                print(
                    await document.setBoard({
                        width: requiredNumber(parsed, '--width'),
                        depth: requiredNumber(parsed, '--depth'),
                        thickness: requiredNumber(parsed, '--thickness'),
                    }),
                );
            });
        case 'panel':
            return editDocument(args.slice(1), 'panel <file>', ['--rows', '--columns', '--gap-x', '--gap-y'], async (document, parsed) => {
                print(
                    await document.setPanel({
                        rows: integer(parsed, '--rows'),
                        columns: integer(parsed, '--columns'),
                        gapX: number(parsed, '--gap-x') ?? 0,
                        gapY: number(parsed, '--gap-y') ?? 0,
                    }),
                );
            });
        case 'component':
            return componentCommand(args.slice(1));
        case 'feeder':
            return feederCommand(args.slice(1));
        case 'profile':
            return profileCommand(args.slice(1));
        case 'validate':
            return readDocument(args.slice(1), 'validate <file>', [], async (document) => {
                print(await document.validate());
            });
        case 'placement':
            return readDocument(
                args.slice(1),
                'placement <file>',
                ['--out'],
                async (document, parsed) => {
                    const report = await document.placement(parsed.flags.has('--optimize'));
                    const target = parsed.options.get('--out');
                    if (target !== undefined) {
                        fs.writeFileSync(target, JSON.stringify(report, null, 2));
                    }
                    print(report);
                },
                ['--optimize'],
            );
        case 'feeders':
            return readDocument(args.slice(1), 'feeders <file>', ['--out'], async (document, parsed) => {
                const report = await document.feederSetup();
                const target = parsed.options.get('--out');
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
        case 'balance':
            return readDocument(
                args.slice(1),
                'balance <file>',
                ['--machines'],
                async (document, parsed) => {
                    printBalance(await document.balance(integer(parsed, '--machines'), parsed.flags.has('--optimize')));
                },
                ['--optimize'],
            );
        case 'reflow':
            return readDocument(args.slice(1), 'reflow <file>', [], async (document) => {
                print(await document.reflow());
            });
        case 'export':
            return readDocument(args.slice(1), 'export <file> --format <format> --out <path>', ['--format', '--out'], async (document, parsed) => {
                const format = exportFormat(parsed.options.get('--format') ?? 'program');
                print(await document.exportDocument(format, required(parsed, '--out')));
            });
        case 'report':
            return readDocument(args.slice(1), 'report <file>', [], async (document) => {
                print(await document.report());
            });
        default:
            throw new OpenSMTError('cli/unknown-command', `unknown command \`${command}\``);
    }
}

async function componentCommand(args: readonly string[]): Promise<number> {
    const action = args[0] ?? '';
    if (action === 'add') {
        return editDocument(
            args.slice(1),
            'component add <file>',
            ['--refdes', '--package', '--x', '--y', '--side', '--rotation', '--height'],
            async (document, parsed) => {
                const height = number(parsed, '--height');
                print(
                    await document.addComponent({
                        refdes: required(parsed, '--refdes'),
                        package: required(parsed, '--package'),
                        side: side(parsed.options.get('--side') ?? 'top'),
                        x: requiredNumber(parsed, '--x'),
                        y: requiredNumber(parsed, '--y'),
                        rotation: number(parsed, '--rotation') ?? 0,
                        height,
                    }),
                );
            },
        );
    }
    if (action === 'remove') {
        return editDocument(args.slice(1), 'component remove <file>', ['--refdes'], async (document, parsed) => {
            print(await document.removeComponent(required(parsed, '--refdes')));
        });
    }
    throw failure('cli/missing-argument', 'missingArgument', { usage: 'component add|remove <file> ...' });
}

async function feederCommand(args: readonly string[]): Promise<number> {
    const action = args[0] ?? '';
    if (action === 'add') {
        return editDocument(
            args.slice(1),
            'feeder add <file>',
            ['--id', '--package', '--pitch', '--capacity', '--loaded'],
            async (document, parsed) => {
                print(
                    await document.addFeeder({
                        id: required(parsed, '--id'),
                        package: required(parsed, '--package'),
                        pitch: requiredNumber(parsed, '--pitch'),
                        capacity: integer(parsed, '--capacity'),
                        loaded: number(parsed, '--loaded'),
                    }),
                );
            },
        );
    }
    if (action === 'remove') {
        return editDocument(args.slice(1), 'feeder remove <file>', ['--id'], async (document, parsed) => {
            print(await document.removeFeeder(required(parsed, '--id')));
        });
    }
    throw failure('cli/missing-argument', 'missingArgument', { usage: 'feeder add|remove <file> ...' });
}

async function profileCommand(args: readonly string[]): Promise<number> {
    if (args[0] !== 'set') {
        throw failure('cli/missing-argument', 'missingArgument', { usage: 'profile set <file> ...' });
    }
    return editDocument(args.slice(1), 'profile set <file>', ['--speed', '--liquidus', '--zones'], async (document, parsed) => {
        print(
            await document.setProfile({
                conveyorSpeed: requiredNumber(parsed, '--speed'),
                liquidus: requiredNumber(parsed, '--liquidus'),
                zones: parseZones(required(parsed, '--zones')),
            }),
        );
    });
}

async function editDocument(
    args: readonly string[],
    usage: string,
    valueOptions: readonly string[],
    work: (document: SmtDocument, parsed: Parsed) => Promise<void>,
    flagOptions: readonly string[] = [],
): Promise<number> {
    const parsed = parse(args, valueOptions, flagOptions);
    const file = positional(parsed, 0, usage);
    return withSession(async (session) => {
        const document = await session.open(file);
        await work(document, parsed);
        return 0;
    });
}

async function readDocument(
    args: readonly string[],
    usage: string,
    valueOptions: readonly string[],
    work: (document: SmtDocument, parsed: Parsed) => Promise<void>,
    flagOptions: readonly string[] = [],
): Promise<number> {
    return editDocument(args, usage, valueOptions, work, flagOptions);
}

const args = process.argv.slice(2);
dispatch(args)
    .then((code) => {
        process.exitCode = code;
    })
    .catch((error: unknown) => {
        if (error instanceof OpenSMTError) {
            process.stderr.write(`${error.code}: ${error.message}\n`);
            process.exitCode = error.code.startsWith('capability/') ? 2 : 1;
        } else {
            process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
            process.exitCode = 1;
        }
    });
