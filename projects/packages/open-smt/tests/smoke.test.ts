import assert from 'node:assert/strict';
import { test } from 'node:test';

import { OpenSMT, OpenSMTError, errorCodes, isOpenSMTError } from '../src/index.js';

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
});
