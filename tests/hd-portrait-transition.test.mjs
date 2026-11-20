import test from 'node:test';
import assert from 'node:assert/strict';
import {PortraitFrameGate} from '../web/hd-portrait-gate.mjs';
import {LiveHdPortraits} from '../web/hd-portrait-live.mjs';
import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('newly matched unready face holds the previous HD frame, then presents the finished portrait',()=>{
  const gate=new PortraitFrameGate({pendingFrames:12,transitionFrames:3});
  assert.equal(gate.shouldDefer(1,{enabled:true,pending:true,key:'a'}),false,
    'never present a blank first frame when no prior picture exists');
  gate.didPresent();
  assert.equal(gate.shouldDefer(2,{enabled:true,pending:true,key:'a'}),true);
  assert.equal(gate.shouldDefer(3,{enabled:true,pending:true,key:'a'}),true);
  assert.equal(gate.shouldDefer(4,{enabled:true,pixels:2200,key:'a'}),false);
  gate.didPresent();
  assert.equal(gate.isHolding,false);
});

test('portrait held frame never freezes indefinitely or for an absent asset',()=>{
  const gate=new PortraitFrameGate({pendingFrames:4,transitionFrames:2});
  gate.didPresent();
  for(let frame=10;frame<14;frame++)
    assert.equal(gate.shouldDefer(frame,{enabled:true,pending:true,key:'missing'}),true);
  assert.equal(gate.shouldDefer(14,{enabled:true,pending:true,key:'missing'}),false);
  assert.equal(gate.shouldDefer(15,{enabled:true,pending:true,key:'missing'}),false);
  assert.equal(gate.shouldDefer(16,{enabled:true,pending:false,ready:false,key:''}),false);
  assert.equal(gate.shouldDefer(17,{enabled:false,pending:true,key:'missing'}),false);
  assert.equal(gate.heldFramesTotal,0);
});

test('a previously successful HD face gets a short handoff; unrelated scenes never hold',()=>{
  const gate=new PortraitFrameGate({pendingFrames:12,transitionFrames:2});
  gate.didPresent();
  assert.equal(gate.shouldDefer(50,{enabled:true,pixels:250,ready:true}),false);
  gate.didPresent();
  assert.equal(gate.shouldDefer(51,{enabled:true,ready:false}),true);
  assert.equal(gate.shouldDefer(52,{enabled:true,ready:false}),true);
  assert.equal(gate.shouldDefer(53,{enabled:true,ready:false}),false);
  assert.equal(gate.shouldDefer(54,{enabled:true,ready:false}),false);
});

test('class1 face requests run in the same emulator update, before displaying the game picture',()=>{
  const app=read('web/app.mjs');
  assert.ok(app.indexOf('livePortraits.pulse(true)')>app.indexOf('game.tick(held)'));
  assert.ok(app.indexOf('livePortraits.pulse(true)')<app.indexOf('drawFrame();'));
  assert.match(app,/if \(holdReasons\.length===0\)/);
  assert.match(app,/portraitFrameGate\.reset\(\)/);
  const ci=read('.github/workflows/web-poc.yml');
  assert.match(ci,/node --test tests\/\*\.test\.mjs/);
});

test('portrait HD can safely request new image in same frame with no second ZIP decode',async()=>{
  let matches=[{index:4,art:'class1/a.png',id:'a',ready:false}];
  const game={
    hd_mode(){return true;},set_portrait_index(){},
    portrait_matches(){return JSON.stringify(matches);},
    portrait_probe(){return '{"candidate":1}'},
    hd_portrait_pixels(){return 0;},
    set_portrait_art(){matches=[{...matches[0],ready:true}]}
  };
  let decodes=0;
  const bridge=new LiveHdPortraits(()=>{},async()=>{
    decodes++;
    return {width:64,height:64,rgba:new Uint8Array(64*64*4)};
  });
  bridge.setCatalog({items:[{id:'a'}]},{readImage(){}});
  bridge.attach(game);
  bridge.pulse(true);
  bridge.pulse(true);
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(decodes,1);
  assert.equal(matches[0].ready,true);
  assert.equal(bridge.loading.size,0);
});
