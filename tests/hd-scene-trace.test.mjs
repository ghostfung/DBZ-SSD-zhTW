import test from 'node:test';
import assert from 'node:assert/strict';
import {HdSceneTrace} from '../web/hd-coverage-runtime.mjs';

const makeGame=()=>{
  let frame=1,hd=true,pose='battle-raditz-001',ready=true,pixels=388;
  const game={
    hd_active:()=>hd,frame_count:()=>frame,
    fighter_matches:()=>JSON.stringify(pose ? [{
      id:pose,key:'abcdef0123456789',ready,luma:false
    }] : []),
    fighter_probe:()=>JSON.stringify({
      poses:1,keyHits:pose?1:0,paletteRejected:0,matched:pose?1:0
    }),
    hd_fighter_pixels:()=>pixels,
    portrait_matches:()=>JSON.stringify([]),
    portrait_probe:()=>JSON.stringify({
      candidate:2,matched:0,weak:1,paletteRejected:1,
      sourceWriteRejected:0,reason:'來源圖塊存在，但調色盤不吻合'
    }),
    hd_portrait_pixels:()=>0,
    special_matches:()=>JSON.stringify([]),
    special_probe:()=>JSON.stringify({poses:0,keyHits:0}),
    hd_special_pixels:()=>0
  };
  return {game,update:v=>{
    if(v.frame!==undefined)frame=v.frame;
    if(v.pose!==undefined)pose=v.pose;
    if(v.pixels!==undefined)pixels=v.pixels;
    if(v.ready!==undefined)ready=v.ready;
    if(v.hd!==undefined)hd=v.hd;
  }};
};

test('opening fighter HD-to-raw regression records a matched-to-unmatched transition',()=>{
  const source=makeGame(),trace=new HdSceneTrace({stepFrames:6,maxSamples:5});
  assert.equal(trace.capture(source.game),true);
  source.update({frame:2});
  assert.equal(trace.capture(source.game),false,'10Hz sampling avoids per-frame JSON work');
  source.update({frame:7,pose:'',pixels:0});
  assert.equal(trace.capture(source.game),true);
  const samples=trace.report().samples;
  assert.equal(samples.length,2);
  assert.equal(samples[0].fighter.items[0].id,'battle-raditz-001');
  assert.equal(samples[0].fighter.pixels,388);
  assert.equal(samples[1].fighter.matched,0);
  assert.equal(samples[1].fighter.pixels,0);
  assert.equal(samples[1].portrait.probe.paletteRejected,1);
  assert.equal(JSON.stringify(trace.report()).includes('rgba'),false);
});

test('trace is local, bounded, resettable, and refuses unstarted/disabled HD',()=>{
  assert.throws(()=>new HdSceneTrace({stepFrames:0}),/limits/);
  const t=new HdSceneTrace({stepFrames:1,maxSamples:3});
  const source=makeGame();
  for(let frame=1;frame<=7;frame++){
    source.update({frame});
    t.capture(source.game);
  }
  assert.deepEqual(t.report().samples.map(v=>v.frame),[5,6,7]);
  source.update({frame:2});
  assert.equal(t.capture(source.game),true);
  assert.deepEqual(t.report().samples.map(v=>v.frame),[2]);
  source.update({frame:3,hd:false});
  assert.equal(t.capture(source.game),false);
  assert.equal(t.report().samples.length,1);
  t.reset();
  assert.deepEqual(t.report().samples,[]);
});

test('missing WASM diagnostic methods are reported as unknown, never a fake zero',()=>{
  const t=new HdSceneTrace();
  t.capture({hd_active:()=>true,frame_count:()=>10});
  const item=t.report().samples[0];
  assert.equal(item.fighter.matched,null);
  assert.equal(item.fighter.pixels,null);
  assert.equal(item.fighter.probe,null);
});
