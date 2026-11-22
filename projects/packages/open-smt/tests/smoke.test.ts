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
