import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
    parseRuntimeStateText,
    processStartTimeFromStat,
    runtimeStateOwnerIsCurrent,
} from './protocol.js';

const fixture = name => readFileSync(
    new URL(`../tests/fixtures/protocol/${name}`, import.meta.url),
    'utf8'
);
const valid = fixture('runtime-valid.json');

test('accepts protocol v1 runtime state with owner identity', () => {
    assert.deepEqual(parseRuntimeStateText(valid), {
        state: 'recording',
        ownerPid: 4242,
        ownerStartTime: 987654,
    });
});

test('requires owner start time', () => {
    assert.equal(parseRuntimeStateText(fixture('runtime-missing-owner-start-time.json')), null);
});

test('invalid documents degrade to idle', () => {
    for (const name of [
        'runtime-malformed.json',
        'runtime-unknown-state.json',
        'runtime-unsupported-version.json',
    ]) assert.equal(parseRuntimeStateText(fixture(name)), null);
});

test('reads field 22 from proc stat even when the command contains spaces', () => {
    const stat = '4242 (codex voice) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 987654 20';
    assert.equal(processStartTimeFromStat(stat), '987654');
    assert.equal(runtimeStateOwnerIsCurrent(parseRuntimeStateText(valid), stat), true);
    assert.equal(runtimeStateOwnerIsCurrent(parseRuntimeStateText(valid), stat.replace('987654', '987655')), false);
});
