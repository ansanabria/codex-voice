import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseRuntimeStateText } from './protocol.js';

const fixture = name => readFileSync(new URL(`../tests/fixtures/protocol/${name}`, import.meta.url), 'utf8');

test('accepts protocol v1 runtime state', () => assert.equal(parseRuntimeStateText(fixture('runtime-valid.json')), 'recording'));
test('unknown state degrades to idle', () => assert.equal(parseRuntimeStateText(fixture('runtime-unknown-state.json')), 'idle'));
test('malformed input degrades to idle', () => assert.equal(parseRuntimeStateText(fixture('runtime-malformed.json')), 'idle'));
test('unsupported version degrades to idle', () => assert.equal(parseRuntimeStateText(fixture('runtime-unsupported-version.json')), 'idle'));
