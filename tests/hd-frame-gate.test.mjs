import test from 'node:test';
import assert from 'node:assert/strict';
import {HdFrameGate} from '../web/hd-frame-gate.mjs';

test('hold the previous HD picture during unknown transition, then immediately resume when HD arrives',()=>{
  const gate=new HdFrameGate({unknownFrames:12,pendingFrames:60});
  assert.equal(gate.shouldDefer(1,{enabled:true,pixels:0}),false);
  gate.didPresent(1400);
  for(let frame=2;frame<=5;frame++)
    assert.equal(gate.shouldDefer(frame,{enabled:true,pixels:0}),true);
  assert.equal(gate.shouldDefer(6,{enabled:true,pixels:1400}),false);
  gate.didPresent(1400);
  assert.equal(gate.heldFramesTotal,4);
  assert.equal(gate.isHolding,false);
});

test('matched but unready local art gets a longer bounded grace period',()=>{
  const gate=new HdFrameGate({unknownFrames:3,pendingFrames:8});
  gate.didPresent(600);
  assert.equal(gate.shouldDefer(100,{enabled:true,pixels:0}),true);
  assert.equal(gate.shouldDefer(102,{enabled:true,pixels:0,pending:true}),true);
  assert.equal(gate.shouldDefer(107,{enabled:true,pixels:0,pending:true}),true);
  assert.equal(gate.shouldDefer(108,{enabled:true,pixels:0,pending:true}),false);
  assert.equal(gate.shouldDefer(109,{enabled:true,pixels:0,pending:true}),false,
    'expired transition may not freeze again indefinitely');
  gate.didPresent(0);
  assert.equal(gate.shouldDefer(110,{enabled:true,pixels:0,pending:true}),false);
  assert.equal(gate.shouldDefer(111,{enabled:true,pixels:400}),false);
  gate.didPresent(400);
  assert.equal(gate.shouldDefer(112,{enabled:true,pixels:0,pending:true}),true);
});

test('ordinary non-HD screens are not held, even when a prior ROM frame exists',()=>{
  const gate=new HdFrameGate();
  gate.didPresent(0);
  assert.equal(gate.shouldDefer(33,{enabled:true,pixels:0,pending:false}),false);
  assert.equal(gate.shouldDefer(34,{enabled:false,pixels:0,pending:true}),false);
  assert.equal(gate.shouldDefer(35,{enabled:true,pixels:0,pending:true}),false);
});

test('a resize, saved state or disabling HD resets the presentation gate',()=>{
  const gate=new HdFrameGate();
  gate.didPresent(1000);
  assert.equal(gate.shouldDefer(91,{enabled:true,pixels:0}),true);
  gate.reset();
  assert.equal(gate.shouldDefer(1,{enabled:true,pixels:0,pending:true}),false);
  assert.equal(gate.heldFramesTotal,0);
});
