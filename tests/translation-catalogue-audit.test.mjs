import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const source=readFileSync(new URL('../translations/zh-tw/zh-tw.tsv',import.meta.url),'utf8');
const shipped=readFileSync(new URL('../web/zh-tw.tsv',import.meta.url),'utf8');
const rows=source.split(/\r?\n/).filter(line=>line && !line.startsWith('#') && !line.startsWith('key\t'))
 .map(line=>{const [key,zh]=line.split('\t');return {key,zh};});
const entries=new Map(rows.map(({key,zh})=>[key,zh]));
const counts=new Map();
for(const {key} of rows) {
 const kind=key.split(':')[0];
 counts.set(kind,(counts.get(kind)||0)+1);
}

test('Web app ships the identical author-licensed 1,495-entry Traditional Chinese table',()=>{
 assert.equal(shipped,source,'Web and original translations must be byte-identical');
 assert.equal(rows.length,1495);
 assert.equal(entries.size,rows.length,'duplicate keys hide translated entries');
 assert.ok(rows.every(({zh})=>zh?.trim()),'translation cannot be an empty field');
});

test('all source translation families needed by story and battle exist',()=>{
 const expected={help:42,battle:362,str2:567,str3:178,str4:9,str5:2,str6:2,str7:8,
   string:37,unit:104,attack:69,object:40,block:27,ending:14,intro:32,screen:2};
 for(const [family,count] of Object.entries(expected)) {
   assert.equal(counts.get(family),count,'family '+family+' changed; revisit text audit');
 }
 assert.equal(entries.get('string:0'),'悟空');
 assert.equal(entries.get('string:2'),'比克');
 assert.equal(entries.get('unit:0x00'),'悟空');
 assert.equal(entries.get('unit:0x01'),'比克');
 assert.ok(entries.get('block:0x35').includes('戰鬥'));
 assert.ok(entries.get('str2:0x002/p2').includes('悟空'));
 assert.ok(entries.has('battle:88') && entries.has('battle:89'));
 assert.ok(entries.has('screen:chase_damage'));
});

// Static inventory alone does not assert rendered pixels: the bridge must
// exercise the upstream Rust translator with real PPU fixtures separately.
