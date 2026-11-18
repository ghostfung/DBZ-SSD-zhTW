import {readFileSync} from 'node:fs';
import {inspectRom} from '../web/rom-check.mjs';
const path=process.argv[2]; if (!path) throw Error('Pass ROM path');
const buf=readFileSync(path);
const view=buf.buffer.slice(buf.byteOffset,buf.byteOffset+buf.byteLength);
const check=inspectRom(view);
console.log(check);
if (check.kind !== 'jp-rev1' || check.crc !== 'ADE7B968') process.exitCode=1;
