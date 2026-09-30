import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyNsisPayload } from './verify-nsis-payload.mjs';

const original = Buffer.from('MZ\0code\xff__TAURI_BUNDLE_TYPE_VAR_UNK\0data\0__TAURI_BUNDLE_TYPE_VAR_NSS', 'latin1');
const packaged = Buffer.from('MZ\0code\xff__TAURI_BUNDLE_TYPE_VAR_NSS\0data\0__TAURI_BUNDLE_TYPE_VAR_NSS', 'latin1');

test('accepts the Tauri NSIS marker patch without changing input buffers', () => {
  const before = Buffer.from(original);
  verifyNsisPayload(original, packaged);
  assert.deepEqual(original, before);
});

test('rejects an unpatched executable inside the installer', () => {
  assert.throws(() => verifyNsisPayload(original, original));
});

test('rejects modifications outside the bundle marker and truncation', () => {
  const damaged = Buffer.from(packaged);
  damaged[2] ^= 1;
  assert.throws(() => verifyNsisPayload(original, damaged));
  assert.throws(() => verifyNsisPayload(original, packaged.subarray(0, -1)));
});

test('rejects candidates without the expected original marker', () => {
  assert.throws(() => verifyNsisPayload(packaged, packaged));
});

test('permits only the first original marker to change, as Tauri does', () => {
  const built = Buffer.concat([original, original]);
  verifyNsisPayload(built, Buffer.concat([packaged, original]));
  assert.throws(() => verifyNsisPayload(built, Buffer.concat([packaged, packaged])));
});
