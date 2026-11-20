import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {HdPresentationBudget} from '../web/hd-presentation-budget.mjs';
import {parseFighterIndex} from '../web/hd-fighter-index.mjs';
import {parseSpecialIndex} from '../web/hd-special-index.mjs';
const source=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');

test('a moving special cannot hold the last displayed frame indefinitely',()=>{
  const b=new HdPresentationBudget({maxBurst:3,windowFrames:60,maxWindowHeld:8});
  assert.deepEqual([10,11,12,13,14,15,16,17,18,19,20,21]
    .map(f=>b.allow(f,true)),[true,true,true,false,true,true,true,false,true,true,false,false]);
  assert.equal(b.summary().totalHeld,8);
  assert.equal(b.summary().totalBypassed,4);
  b.allow(22,false);
  assert.equal(b.allow(75,true),true,'budget replenishes after one simulated second');
});

test('budget resets on rewind and does not delay unrequested screens',()=>{
  const b=new HdPresentationBudget({maxBurst:2,windowFrames:60,maxWindowHeld:2});
  assert.equal(b.allow(1,false),false);
  assert.equal(b.allow(2,true),true);
  assert.equal(b.allow(3,true),true);
  assert.equal(b.allow(4,true),false);
  assert.equal(b.allow(1,false),false);
  assert.equal(b.summary().totalBypassed,0);
  assert.equal(b.allow(2,true),true);
});

test('only transient matches may request holds; story/background protections remain',()=>{
  const app=source('web/app.mjs');
  assert.match(app,/const transientOnly=holdReasons\.length>0&&!storyHold&&!hdHold/);
  assert.match(app,/if\(bypassedTransient\)holdReasons\.length=0/);
  assert.match(app,/specialMatches\.length>0/);
  assert.match(app,/fighterMatches\.length>0/);
  assert.match(app,/mode7Matches\.length>0/);
  assert.match(app,/event\.detail\.framePerformance=/);
  assert.doesNotMatch(source('web/service-worker.js'),/addEventListener\('fetch'/);
  assert.match(source('crates/ssd-web/src/lib.rs'),/dbz-hd-tw-pwa-v45/);
});

test('official luma effects remain eligible while unverified fighter and class3 art is rejected',()=>{
  const luma={id:'fgfx-luma',source_key:'0011223344556677',
    source_size:[16,16],file:'grey.png',luma:true,palette_indexes_used:{}};
  const fighter=parseFighterIndex({assets_list:[luma]},()=>true);
  const effect=parseSpecialIndex({assets_list:[luma]},2,()=>true);
  const field=parseSpecialIndex({assets_list:[luma]},1,()=>true);
  assert.equal(fighter.items[0].available,false);
  assert.equal(effect.items[0].available,true);
  assert.equal(effect.items[0].luma,true);
  assert.equal(field.items[0].available,false);
  const rust=source('crates/ssd-web/src/live_specials.rs');
  assert.match(rust,/hd_luma::colour\(/,'verified luma stencil must retain original hue');
  assert.match(rust,/if entry\.kind == 2 \{ return true; \}/);
});

test('Safari timing windows are bounded and contain only summary measurements',()=>{
  const app=source('web/app.mjs');
  assert.match(app,/if\(perfHistory\.length>30\)perfHistory\.shift\(\)/);
  assert.match(app,/perfHistory\.push\(\{frame:game\.frame_count\(\),\.\.\.perf\}\)/);
  assert.match(app,/transientHoldBudget\.summary\(\)/);
});
