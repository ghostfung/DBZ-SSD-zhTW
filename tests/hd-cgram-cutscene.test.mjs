import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {HdSceneTrace} from '../web/hd-coverage-runtime.mjs';

const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('class2 RGBA source-key matching survives mid-frame CGRAM updates',()=>{
  const hd=source('crates/ssd-hd/src/lib.rs');
  const match=hd.match(/pub fn live_special_poses\(log: &FrameLog\)[\s\S]*?\n}\n/);
  assert.ok(match,'native special matcher should exist');
  assert.doesNotMatch(match[0],/if !log\.complete[\s\S]*?!log\.cgram_writes\.is_empty\(\)/,
    'CGRAM writes cannot globally reject every RGBA effect');
  assert.match(match[0],/pose\.field[\s\S]*?log\.cgram_writes\.is_empty\(\)/,
    'index-based class3 field art remains conservative');
  assert.match(match[0],/art::poses::poses_in\(log,&sprites,&view\)/,
    'effects must still be keyed using the real upstream PPU source hash');
  const special=source('crates/ssd-web/src/live_specials.rs');
  assert.match(special,/self\.cgram_writes=log\.cgram_writes\.len\(\)/);
  assert.match(special,/"cgramWrites":self\.cgram_writes/);
});

test('local scene diagnostics distinguish CGRAM writes from missing effect keys',()=>{
  const trace=new HdSceneTrace({stepFrames:1,maxSamples:3});
  let frame=1,cgramWrites=0,matches=9,pixels=488;
  const game={
    frame_count:()=>frame,hd_active:()=>true,
    special_matches:()=>JSON.stringify(Array.from({length:matches},(_,i)=>({
      id:'effect-'+i,ready:true,key:'abcdef0123456789',luma:true
    }))),
    special_probe:()=>JSON.stringify({
      poses:12,keyHits:matches,matched:matches,cgramWrites,
    }),
    hd_special_pixels:()=>pixels
  };
  trace.capture(game);
  frame=2;cgramWrites=5;trace.capture(game);
  frame=3;matches=0;pixels=0;trace.capture(game);
  const samples=trace.report().samples.map(v=>v.special);
  assert.deepEqual(samples.map(v=>v.probe.cgramWrites),[0,5,5]);
  assert.deepEqual(samples.map(v=>v.pixels),[488,488,0]);
});
