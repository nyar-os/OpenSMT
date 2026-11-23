import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { OpenSMT, OpenSMTError, errorCodes, isOpenSMTError } from '../src/index.js';
import { nativeBinding } from '../src/native.js';

const nativeMissing = nativeBinding() === undefined;
const skipWithoutNative = nativeMissing ? 'the native addon is not installed' : false;

test('the session stores its locale and starts empty', () => {
    const session = new OpenSMT({ locale: 'zh-CN' });
    assert.equal(session.locale, 'zh-CN');
    assert.equal(session.disposed, false);
});

test('an empty path is rejected before the addon is touched', async () => {
    const session = new OpenSMT();
    await assert.rejects(session.create(''), (error: unknown) => {
        assert.ok(isOpenSMTError(error));
        assert.equal(error.code, 'session/argument-invalid');
        return true;
    });
    await assert.rejects(session.open(''), (error: unknown) => {
        assert.ok(error instanceof OpenSMTError);
        assert.equal(error.code, 'session/argument-invalid');
        return true;
    });
    await session.dispose();
});

test('a disposed session refuses new documents', async () => {
    const session = new OpenSMT();
    await session.dispose();
    await assert.rejects(session.create('panel.osmt'), (error: unknown) => {
        assert.ok(isOpenSMTError(error));
        assert.equal(error.code, 'session/disposed');
        return true;
    });
});

test('the error surface includes coverage and capability codes', () => {
    assert.ok(errorCodes.includes('capability/not-implemented'));
    assert.ok(errorCodes.includes('feeder/missing'));
    assert.ok(errorCodes.includes('feeder/capacity'));
    assert.ok(errorCodes.includes('component/outside-board'));
    assert.ok(errorCodes.includes('balance/invalid-machines'));
    assert.ok(errorCodes.includes('schedule/invalid-strategy'));
    assert.ok(errorCodes.includes('schedule/invalid-lanes'));
    assert.ok(errorCodes.includes('bom/missing-column'));
    assert.ok(errorCodes.includes('bom/duplicate-refdes'));
    assert.ok(errorCodes.includes('takt/invalid'));
    assert.ok(errorCodes.includes('station/invalid'));
    assert.ok(errorCodes.includes('changeover/invalid'));
    assert.ok(errorCodes.includes('reflow/invalid'));
    assert.ok(errorCodes.includes('consumption/invalid'));
    assert.ok(errorCodes.includes('program/invalid'));
});

test('balance splits components and rejects zero machines', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-balance-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'R2', package: '0603', x: 50, y: 10 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });

        const report = await document.balance(2);
        assert.equal(report.format, 'open-smt-balance');
        assert.equal(report.version, 1);
        assert.equal(report.machines.length, 2);
        assert.equal(report.ok, true);
        assert.deepEqual(
            report.machines.flatMap((machine) => machine.components).sort(),
            ['R1', 'R2'],
        );
        assert.equal(report.machines.reduce((sum, machine) => sum + machine.components.length, 0), 2);
        for (const machine of report.machines) {
            assert.equal(machine.feeders.length, 1);
        }

        await assert.rejects(document.balance(0), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'balance/invalid-machines');
            return true;
        });
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('panel fit reports how many boards fit and leaves the document unchanged', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-panel-fit-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        const before = document.revision;

        const report = await document.panelFit({ maxWidth: 300, maxDepth: 160 });
        assert.equal(report.format, 'open-smt-panel-fit');
        assert.equal(report.version, 1);
        assert.equal(report.columns, 3);
        assert.equal(report.rows, 2);
        assert.equal(report.boardCount, 6);
        assert.equal(report.fits, true);
        assert.equal(report.wastedWidth, 0);
        assert.equal(report.wastedDepth, 0);
        assert.equal(report.utilization, 1);
        assert.equal(document.revision, before);

        await assert.rejects(document.panelFit({ maxWidth: 0, maxDepth: 160 }), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'panel/invalid');
            return true;
        });
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('bom check reports missing, extra and quantity findings and leaves the document unchanged', {
    skip: skipWithoutNative,
}, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-bom-'));
    const file = path.join(directory, 'panel.osmt');
    const bomFile = path.join(directory, 'bom.csv');
    const badBomFile = path.join(directory, 'bad-bom.csv');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'R2', package: '0603', x: 20, y: 10 });
        await document.addComponent({ refdes: 'C1', package: '0805', x: 30, y: 10 });
        const before = document.revision;

        await fs.writeFile(bomFile, 'package,quantity,refdes\n0603,3,R1;R2;R3\nQFN-32,1,U1\n');
        const report = await document.bomCheck(bomFile);
        assert.equal(report.format, 'open-smt-bom-check');
        assert.equal(report.version, 1);
        assert.equal(report.ok, false);
        assert.equal(report.bomLines, 2);
        assert.equal(report.components, 3);
        assert.deepEqual(report.missing, [
            { package: '0603', refdes: 'R3' },
            { package: 'QFN-32', refdes: 'U1' },
        ]);
        assert.deepEqual(report.extra, [{ package: '0805', refdes: 'C1' }]);
        assert.deepEqual(report.quantityMismatches, [
            { package: '0603', bomQuantity: 3, placedQuantity: 2, difference: -1 },
            { package: '0805', bomQuantity: 0, placedQuantity: 1, difference: 1 },
            { package: 'QFN-32', bomQuantity: 1, placedQuantity: 0, difference: -1 },
        ]);

        await fs.writeFile(bomFile, 'package,quantity,refdes\n0603,2,R1;R2\n0805,1,C1\n');
        const clean = await document.bomCheck(bomFile);
        assert.equal(clean.ok, true);
        assert.deepEqual(clean.missing, []);
        assert.deepEqual(clean.extra, []);
        assert.deepEqual(clean.quantityMismatches, []);

        await fs.writeFile(badBomFile, 'package,refdes\n0603,R1\n');
        await assert.rejects(document.bomCheck(badBomFile), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'bom/missing-column');
            return true;
        });
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('schedule lanes splits the program and rejects an invalid count', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-lanes-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'R2', package: '0603', x: 50, y: 10 });
        await document.addComponent({ refdes: 'R3', package: '0603', x: 50, y: 50 });

        const report = await document.lanes(3, 'lpt', true);
        assert.equal(report.format, 'open-smt-lanes');
        assert.equal(report.version, 1);
        assert.equal(report.strategy, 'lpt');
        assert.equal(report.laneCount, 3);
        assert.equal(report.lanes.length, 3);
        assert.deepEqual(
            report.lanes.flatMap((lane) => lane.tasks).sort(),
            ['R1', 'R2', 'R3'],
        );
        for (const lane of report.lanes) {
            assert.ok(lane.index < 3);
        }
        assert.ok(Math.abs(report.makespan - Math.max(...report.lanes.map((lane) => lane.seconds))) < 1e-9);
        assert.ok(Math.abs(report.balanceRatio - report.makespan / report.singleLaneSeconds) < 1e-12);

        await assert.rejects(document.lanes(1), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'schedule/invalid-lanes');
            return true;
        });
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('component update edits a placed component in place', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-update-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        await document.addFeeder({ id: 'F2', package: '0805', pitch: 4, capacity: 100 });
        const before = document.revision;

        await document.updateComponent('R1', {
            package: '0805',
            side: 'bottom',
            x: 20,
            y: 30,
            rotation: 450,
        });
        assert.equal(document.revision, before + 1);

        const placement = await document.placement();
        const entry = placement.entries.find((item) => item.refdes === 'R1');
        assert.ok(entry);
        assert.equal(entry.package, '0805');
        assert.equal(entry.side, 'bottom');
        assert.equal(entry.x, 20);
        assert.equal(entry.y, 30);
        assert.equal(entry.rotation, 90);
        assert.equal(entry.feeder, 'F2');

        await assert.rejects(document.updateComponent('R9', { x: 20 }), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'component/unknown-refdes');
            return true;
        });
        await assert.rejects(document.updateComponent('R1', {}), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'component/invalid');
            return true;
        });
        await assert.rejects(document.updateComponent('R1', { x: 500 }), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'component/outside-board');
            return true;
        });
        assert.equal(document.revision, before + 1);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('component list reports document order, package grouping and feeder allocation', {
    skip: skipWithoutNative,
}, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-component-list-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R2', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'R1', package: '0603', x: 20, y: 10, height: 0.8 });
        await document.addComponent({ refdes: 'C1', package: '0805', side: 'bottom', x: 30, y: 10 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 10, loaded: 1 });
        const before = document.revision;

        const report = await document.componentList();
        assert.equal(report.format, 'open-smt-component-list');
        assert.equal(report.version, 1);
        assert.equal(report.units, 'mm');
        assert.equal(report.componentCount, 3);
        assert.equal(report.unassignedCount, 2);
        assert.deepEqual(
            report.components.map((row) => [row.refdes, row.package, row.side, row.feeder]),
            [
                ['R2', '0603', 'top', null],
                ['R1', '0603', 'top', 'F1'],
                ['C1', '0805', 'bottom', null],
            ],
        );
        assert.equal(report.components[1]?.height, 0.8);
        assert.deepEqual(
            report.packages.map((entry) => [entry.package, entry.count, entry.refdes]),
            [
                ['0603', 2, ['R2', 'R1']],
                ['0805', 1, ['C1']],
            ],
        );
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('takt estimates cycle time and splits it over machines', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-takt-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'R2', package: '0603', x: 50, y: 10 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        const before = document.revision;

        const report = await document.takt();
        assert.equal(report.format, 'open-smt-takt');
        assert.equal(report.version, 1);
        assert.equal(report.components, 2);
        assert.equal(report.picks, 2);
        assert.ok(report.travelMm > 0);
        assert.ok(Math.abs(report.placeSeconds - 2 * 0.6) < 1e-9);
        assert.ok(Math.abs(report.boardSeconds - (report.placeSeconds + report.travelSeconds + report.overheadSeconds)) < 1e-9);
        assert.ok(Math.abs(report.boardsPerHour - 3600 / report.boardSeconds) < 1e-9);

        const doubled = await document.takt({ machineCount: 2 });
        assert.ok(Math.abs(doubled.machineSeconds - report.boardSeconds / 2) < 1e-9);
        assert.ok(Math.abs(doubled.boardsPerHour - 2 * report.boardsPerHour) < 1e-9);

        const optimized = await document.takt({ optimize: true });
        assert.equal(optimized.optimized, true);
        assert.equal(typeof optimized.travelSaved, 'number');

        await assert.rejects(document.takt({ headSpeed: 0 }), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'takt/invalid');
            return true;
        });
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('feeder optimize assigns slots by mean placement x and leaves the document unchanged', {
    skip: skipWithoutNative,
}, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-station-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 90, y: 10 });
        await document.addComponent({ refdes: 'C1', package: '0805', x: 10, y: 10 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        await document.addFeeder({ id: 'F2', package: '0805', pitch: 4, capacity: 100 });
        const before = document.revision;

        const report = await document.feederOptimize();
        assert.equal(report.format, 'open-smt-station');
        assert.equal(report.version, 1);
        assert.equal(report.slotCount, 2);
        assert.equal(report.stations[0]?.feederId, 'F2');
        assert.equal(report.stations[0]?.slot, 1);
        assert.equal(report.stations[0]?.positionMm, 0);
        assert.equal(report.stations[1]?.feederId, 'F1');
        assert.equal(report.stations[1]?.positionMm, 16);
        assert.equal(report.travelSaved, 16);
        assert.ok(Math.abs(report.travelSaved - (report.baselineTravelMm - report.travelMm)) < 1e-9);

        await assert.rejects(document.feederOptimize(0), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'station/invalid');
            return true;
        });
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('panel map maps every component into every board instance', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-panel-map-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.setPanel({ rows: 2, columns: 2, gapX: 5, gapY: 4 });
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'C1', package: '0805', side: 'bottom', x: 20, y: 30 });
        const before = document.revision;

        const report = await document.panelMap();
        assert.equal(report.format, 'open-smt-panel-map');
        assert.equal(report.version, 1);
        assert.equal(report.boardCount, 4);
        assert.deepEqual(
            report.instances.map((instance) => [instance.board, instance.row, instance.column, instance.offsetX, instance.offsetY]),
            [
                [1, 1, 1, 0, 0],
                [2, 1, 2, 105, 0],
                [3, 2, 1, 0, 84],
                [4, 2, 2, 105, 84],
            ],
        );
        assert.deepEqual(
            report.components[0]?.instances.map((placement) => [placement.board, placement.x, placement.y]),
            [
                [1, 10, 10],
                [2, 115, 10],
                [3, 10, 94],
                [4, 115, 94],
            ],
        );
        assert.equal(report.ok, true);
        assert.equal(report.outsideBoard, 0);
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('first article builds a check sheet and exports it as csv', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-first-article-'));
    const file = path.join(directory, 'panel.osmt');
    const sheetFile = path.join(directory, 'first-article.csv');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 90, y: 10 });
        await document.addComponent({ refdes: 'R2', package: '0603', x: 10, y: 10, rotation: 90 });
        await document.addComponent({ refdes: 'C1', package: '0805', side: 'bottom', x: 20, y: 20 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        await document.addFeeder({ id: 'F2', package: '0805', pitch: 8, capacity: 50 });
        const before = document.revision;

        const report = await document.firstArticle();
        assert.equal(report.format, 'open-smt-first-article');
        assert.equal(report.version, 1);
        assert.deepEqual(
            report.steps.map((step) => [step.step, step.refdes, step.feeder, step.pitch]),
            [
                [1, 'R2', 'F1', 4],
                [2, 'R1', 'F1', 4],
                [3, 'C1', 'F2', 8],
            ],
        );
        assert.equal(report.topComponents, 2);
        assert.equal(report.bottomComponents, 1);
        assert.deepEqual([...report.checklist], ['placement', 'orientation', 'package', 'feeder']);
        assert.equal(report.materials[0]?.package, '0603');
        assert.equal(report.materials[0]?.lanes[0]?.assigned, 2);

        const exported = await document.exportDocument('fai', sheetFile);
        assert.equal(exported.format, 'fai');
        const csv = await fs.readFile(sheetFile, 'utf8');
        assert.ok(csv.startsWith('step,refdes,package,side,x,y,rotation,feeder,pitch,placement_ok'));
        assert.ok(csv.includes('\n1,R2,0603,top,10,10,90,F1,4,,,,'));
        assert.ok(csv.includes('\n3,C1,0805,bottom,20,20,0,F2,8,,,,'));
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('reflow window compares the profile with limits and leaves the document unchanged', {
    skip: skipWithoutNative,
}, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-reflow-window-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        const before = document.revision;

        const report = await document.reflowWindow({
            peakMin: 235,
            peakMax: 250,
            timeAboveLiquidusMin: 15,
            timeAboveLiquidusMax: 60,
            maxHeatingRate: 5,
            maxCoolingRate: 6,
        });
        assert.equal(report.format, 'open-smt-reflow-window');
        assert.equal(report.version, 1);
        assert.equal(report.temperatureUnit, 'celsius');
        assert.equal(report.peakTemperature, 245);
        assert.equal(report.totalSeconds, 140);
        assert.equal(report.zones.length, 4);
        assert.ok(Math.abs(report.zones[0]!.rampRate - 125 / 30) < 1e-9);
        assert.ok(Math.abs(report.zones[3]!.rampRate + 215 / 40) < 1e-9);
        assert.ok(Math.abs(report.maxHeatingRate - 125 / 30) < 1e-9);
        assert.ok(Math.abs(report.maxCoolingRate - 5.375) < 1e-9);
        assert.equal(report.checks.length, 6);
        assert.ok(Math.abs(report.checks[0]!.margin - 10) < 1e-9);
        assert.ok(report.ok);

        const failing = await document.reflowWindow({ maxHeatingRate: 4 });
        assert.equal(failing.checks.length, 1);
        assert.ok(failing.checks[0]!.margin < 0);
        assert.equal(failing.ok, false);

        await assert.rejects(document.reflowWindow({ peakMin: 300, peakMax: 200 }), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'reflow/invalid');
            return true;
        });
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('feeder consumption plans reels for a build quantity', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-consumption-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'R2', package: '0603', x: 50, y: 10 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 10 });
        const before = document.revision;

        const report = await document.consumption(5);
        assert.equal(report.format, 'open-smt-consumption');
        assert.equal(report.version, 1);
        assert.equal(report.boards, 5);
        assert.equal(report.componentsPerBoard, 2);
        assert.equal(report.totalPicks, 10);
        assert.equal(report.feeders[0]?.picksPerBoard, 2);
        assert.equal(report.feeders[0]?.picks, 10);
        assert.equal(report.feeders[0]?.reels, 1);
        assert.equal(report.feeders[0]?.remainder, 10);
        assert.equal(report.feeders[0]?.short, false);
        assert.equal(report.packages[0]?.reels, 1);
        assert.ok(report.ok);

        const beyond = await document.consumption(6);
        assert.equal(beyond.feeders[0]?.picks, 12);
        assert.equal(beyond.feeders[0]?.reels, 2);
        assert.equal(beyond.feeders[0]?.remainder, 2);
        assert.equal(beyond.feeders[0]?.short, true);
        assert.equal(beyond.ok, false);

        await assert.rejects(document.consumption(0), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'consumption/invalid');
            return true;
        });
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('takt heatmap attributes step time to packages and sides', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-heatmap-'));
    const file = path.join(directory, 'panel.osmt');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'C1', package: '0805', x: 10, y: 50 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        await document.addFeeder({ id: 'F2', package: '0805', pitch: 8, capacity: 50 });
        const before = document.revision;

        const report = await document.taktHeatmap();
        assert.equal(report.format, 'open-smt-heatmap');
        assert.equal(report.version, 1);
        assert.equal(report.components, 2);
        assert.equal(report.steps.length, 2);
        assert.equal(report.steps[0]?.refdes, 'R1');
        assert.equal(report.steps[1]?.refdes, 'C1');
        assert.ok(Math.abs(report.steps[0]!.travelMm - Math.SQRT2 * 10) < 1e-9);
        assert.ok(Math.abs(report.steps[1]!.travelMm - 40) < 1e-9);
        assert.equal(report.bottleneck, '0805');
        assert.deepEqual(
            report.packages.map((entry) => entry.package),
            ['0805', '0603'],
        );
        const expected = 0.6 + (Math.SQRT2 * 10) / 200 + 0.8;
        assert.ok(Math.abs(report.totalSeconds - expected) < 1e-12);
        const takt = await document.takt();
        assert.ok(Math.abs(report.totalSeconds - takt.boardSeconds) < 1e-9);
        assert.ok(Math.abs(report.packages[1]!.cumulativeShare - 1) < 1e-9);
        assert.equal(document.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('machine export writes a readable program and verify checks it back', { skip: skipWithoutNative }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-machine-'));
    const file = path.join(directory, 'panel.osmt');
    const programFile = path.join(directory, 'machine.csv');
    const session = new OpenSMT();
    try {
        const document = await session.create(file);
        await document.addComponent({ refdes: 'R1', package: '0603', x: 10, y: 10 });
        await document.addComponent({ refdes: 'C1', package: '0805', side: 'bottom', x: 20, y: 20 });
        await document.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        await document.addFeeder({ id: 'F2', package: '0805', pitch: 8, capacity: 50 });
        const before = document.revision;

        const exported = await document.machineExport(programFile, { '0603': 'N1', '0805': 'N2' });
        assert.equal(exported.format, 'open-smt-machine-program');
        assert.equal(exported.steps, 2);
        assert.equal(exported.verified, true);
        assert.equal(exported.bytes, (await fs.readFile(programFile, 'utf8')).length);

        const csv = await fs.readFile(programFile, 'utf8');
        assert.ok(csv.startsWith('sequence,side,refdes,package,x_mm,y_mm,rotation_deg,feeder,station,pitch_mm,nozzle,speed_mm_s\n'));
        assert.ok(csv.includes('\n1,top,R1,0603,10,10,0,F1,1,4,N1,200\n'));
        assert.ok(csv.includes('\n2,bottom,C1,0805,20,20,0,F2,2,8,N2,200\n'));

        const verified = await document.machineVerify(programFile, { '0603': 'N1', '0805': 'N2' });
        assert.equal(verified.ok, true);
        assert.deepEqual(verified.mismatches, []);
        assert.equal(verified.steps, 2);

        await fs.writeFile(programFile, csv.replace('10,10,0,F1', '11,10,0,F1'));
        const tampered = await document.machineVerify(programFile, { '0603': 'N1', '0805': 'N2' });
        assert.equal(tampered.ok, false);
        assert.equal(tampered.mismatches[0]?.field, 'x');
        assert.equal(document.revision, before);

        await fs.writeFile(programFile, 'wrong,header\n');
        await assert.rejects(document.machineVerify(programFile), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'program/invalid');
            return true;
        });
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});

test('changeover orders removals, swaps and additions between two documents', {
    skip: skipWithoutNative,
}, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'open-smt-changeover-'));
    const currentFile = path.join(directory, 'current.osmt');
    const nextFile = path.join(directory, 'next.osmt');
    const session = new OpenSMT();
    try {
        const current = await session.create(currentFile);
        await current.addFeeder({ id: 'F1', package: '0603', pitch: 4, capacity: 100 });
        await current.addFeeder({ id: 'F2', package: '0805', pitch: 4, capacity: 100 });
        const next = await session.create(nextFile);
        await next.addFeeder({ id: 'F2', package: '1206', pitch: 4, capacity: 100 });
        await next.addFeeder({ id: 'F3', package: 'QFN-32', pitch: 12, capacity: 50 });
        const before = current.revision;

        const report = await current.changeover(nextFile);
        assert.equal(report.format, 'open-smt-changeover');
        assert.equal(report.version, 1);
        assert.deepEqual(
            report.actions.map((action) => [action.kind, action.id]),
            [
                ['remove', 'F1'],
                ['swap', 'F2'],
                ['add', 'F3'],
            ],
        );
        assert.deepEqual([...report.unchanged], []);
        assert.equal(report.changes, 4);
        assert.equal(report.estimatedSeconds, 120);
        assert.equal(report.actions[1]?.toPackage, '1206');

        const fast = await current.changeover(nextFile, 10);
        assert.equal(fast.estimatedSeconds, 40);

        await assert.rejects(current.changeover(nextFile, 0), (error: unknown) => {
            assert.ok(isOpenSMTError(error));
            assert.equal(error.code, 'changeover/invalid');
            return true;
        });
        assert.equal(current.revision, before);
    } finally {
        await session.dispose();
        await fs.rm(directory, { recursive: true, force: true });
    }
});
