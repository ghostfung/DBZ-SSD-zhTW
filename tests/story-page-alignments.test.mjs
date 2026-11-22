import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const get=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
test('iPhone keeps the original author-licensed 1,495-entry table untouched',()=>{
 assert.equal(get('web/zh-tw.tsv'),get('translations/zh-tw/zh-tw.tsv'));
});
test('only stage-proven Raditz story pages receive supplemental alignments',()=>{
 const source=get('translations/zh-tw/zh-tw.tsv');
 const corrections=get('web/zh-tw-page-alignments.tsv');
 const entries=corrections.split('\n').filter(x=>x && !x.startsWith('#')).map(x=>x.split('\t'));
 assert.deepEqual(entries.map(x=>x[0]),['str3:0x3F/p2','str3:0x3F/p3']);
 assert.ok(source.includes('str3:0x3F/p2\t究竟能不能救出悟飯！？'));
 assert.ok(!source.includes('str3:0x3F/p3\t'));
 assert.equal(entries[1][1],'究竟能不能救出悟飯！？');
 assert.match(get('web/app.mjs'),/fetch\('\.\/zh-tw-page-alignments\.tsv'\)/);
 assert.ok(get('web/service-worker.js').includes("url.pathname.endsWith('/pkg/ssd_web_bg.wasm')"));
});
