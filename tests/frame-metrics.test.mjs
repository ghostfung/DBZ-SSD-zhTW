import test from 'node:test';
import assert from 'node:assert/strict';
import {FrameMetrics} from '../web/frame-metrics.mjs';
test('frame meter reports rare event-loop stalls even when average game FPS looks fine',()=>{
 const meter=new FrameMetrics(200);
 for(let i=0;i<9;i++)assert.equal(meter.add({now:i*20,gap:16.6,tickMs:4,drawMs:2,ticks:1}),null);
 const sample=meter.add({now:200,gap:120,tickMs:42,drawMs:8,ticks:2});
 assert.equal(sample.samples,10);
 assert.equal(sample.maxGap,120);
 assert.equal(sample.maxTick,42);
 assert.equal(sample.slow,1);
 assert.equal(sample.catchups,1);
 assert.match(FrameMetrics.describe(sample),/最高 120.0 ms/);
 assert.equal(meter.count,0);
});
test('meter does not require rendering on RAF callbacks without simulation steps',()=>{
 const meter=new FrameMetrics(100);
 assert.equal(meter.add({now:0,gap:0,ticks:0}),null);
 assert.equal(meter.add({now:100,gap:16,ticks:0}).samples,2);
});
