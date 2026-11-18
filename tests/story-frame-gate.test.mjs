import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {StoryFrameGate} from '../web/story-frame-gate.mjs';

test('do not delay initial boot, menu or non-HD frames',()=>{
  const gate=new StoryFrameGate(10);
  assert.equal(gate.shouldDefer(1,true,true),false); // no previous frame
  gate.didPresent();
  assert.equal(gate.shouldDefer(2,false,true),false);
  assert.equal(gate.shouldDefer(3,false,false),false);
  assert.equal(gate.isHolding,false);
});

test('hold the last translated image for at most ten simulated frames; never block emulation',()=>{
  const gate=new StoryFrameGate(10);
  gate.didPresent();
  for(let frame=100;frame<110;frame++){
    assert.equal(gate.shouldDefer(frame,true,true),true,'frame '+frame);
  }
  assert.equal(gate.heldFramesTotal,10);
  assert.equal(gate.shouldDefer(110,true,true),false,'fail open if translation never arrives');
  assert.equal(gate.shouldDefer(111,false,true),false);
  assert.equal(gate.shouldDefer(112,true,true),true,'next transition gets a fresh budget');
});

test('reset after loading a save or toggling HD; frame counter may restart',()=>{
  const gate=new StoryFrameGate(10);
  gate.didPresent();
  assert.equal(gate.shouldDefer(200,true,true),true);
  gate.reset();
  assert.equal(gate.shouldDefer(1,true,true),false);
  assert.equal(gate.heldFramesTotal,0);
  gate.didPresent();
  assert.equal(gate.shouldDefer(2,true,false),false);
  assert.equal(gate.shouldDefer(3,true,true),true);
});

test('the hold signal is based on PPU story cells + no translated pixel samples, not colour-rectangle guesses',()=>{
  const src=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
  const rust=src('crates/ssd-web/src/lib.rs');
  const renderer=src('crates/ssd-hd/src/lib.rs');
  const app=src('web/app.mjs');
  assert.match(rust,/self\.native_story_ascii_cells > 0/);
  assert.match(rust,/self\.hd_renderer\.native_changed_samples\(\) == 0/);
  assert.match(renderer,/page\.table != 3/);
  assert.match(renderer,/self\.text\.cells\(\)\.iter\(\)\.filter/);
  assert.match(app,/game\.story_translation_pending\(\)/);
  assert.match(app,/storyFrameGate\.shouldDefer\(/);
  assert.doesNotMatch(app,/maskPendingDialogue|drawChineseInBox|findDialogueBox/);
});
