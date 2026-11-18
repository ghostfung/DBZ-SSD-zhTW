import assert from 'node:assert/strict';
import {crc32,crcHex,inspectRom} from '../web/rom-check.mjs';
assert.equal(crcHex(crc32(new TextEncoder().encode('123456789'))),'CBF43926');
assert.equal(inspectRom(new Uint8Array(10).buffer).kind,'invalid');
assert.equal(inspectRom(new Uint8Array(32768).buffer).kind,'unknown');
console.log('CRC32 and ROM size-check tests OK');
