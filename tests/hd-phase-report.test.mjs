import test from 'node:test';
import assert from 'node:assert/strict';
import {describeHdPhases} from '../web/hd-phase-report.mjs';
test('phase report displays real profiler averages and local read ahead statistics',()=>{
 const stages=Object.fromEntries(['emulation','ppu','background','map','mode7',
  'fighters','portrait','cards','translation'].map(n=>[n,{avg:1.5,max:4}]));
 const text=describeHdPhases({samples:8,stages},{
    metrics:{prefetchCount:2,prefetchHits:1,prefetchMs:19.4}
 });
 assert.match(text,/8 個樣本/);
 assert.match(text,/PPU 2x 1.5/);
 assert.match(text,/地圖預讀 2、命中 1、預讀耗時 19.4 ms/);
});
test('no profiler results do not display fabricated time',()=>{
 assert.match(describeHdPhases({samples:0,stages:{}}),/尚未有足夠樣本/);
 assert.match(describeHdPhases('{oops'),/資料錯誤/);
});
