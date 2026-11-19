import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {parseMapIndex} from '../web/hd-map-index.mjs';
import {parseSpecialIndex} from '../web/hd-special-index.mjs';
import {parseMode7Index} from '../web/hd-mode7-index.mjs';
import {parseCardIndex} from '../web/hd-card-index.mjs';
import {PortraitFrameGate} from '../web/hd-portrait-gate.mjs';
const source=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
test('maps need original block dimensions and exact ROM pointer layout',()=>{
  const entry={id:'map-01',maps:[2],blocks:[3,4],grid:'06:F360',
    metatiles:'09:EFB0',tiles:'map-01/t{x}_{y}.png'};
  const res=parseMapIndex({version:1,scale:4,tile:256,layers:[entry]});
  assert.equal(res.count,1);assert.equal(res.items[0].tiles,'maps/map-01/t{x}_{y}.png');
  const bad=parseMapIndex({version:1,scale:4,tile:256,layers:[
    {...entry,grid:'../xx'}, {...entry,tiles:'../x{x}{y}.png'},
    {...entry,blocks:[99,1]}]});
  assert.equal(bad.count,0);
  assert.equal(bad.ignored,3);
  assert.throws(()=>parseMapIndex({version:1,scale:2,tile:256,layers:[]}),/格式/);
});
test('class2 effects and class3 objects never share namespaced source keys',()=>{
  const img={id:'effect',source_key:'abcd1234abcd1234',
    source_size:[32,32],file:'effect.webp',
    palette_indexes_used:{'1':'#FF0000','2':'#00FF00'}};
  let result=parseSpecialIndex({assets_list:[img]},2,()=>true);
  assert.equal(result.count,1);assert.equal(result.items[0].kind,2);
  assert.equal(result.items[0].art,'class2/effect.webp');
  result=parseSpecialIndex({assets_list:[img]},1,()=>true);
  assert.equal(result.items[0].kind,1);
  assert.equal(result.items[0].art,'class3/effect.webp');
  const invalid=parseSpecialIndex({assets_list:[{...img,file:'../hack.png'}]},2);
  assert.equal(invalid.count,0);
});
test('cards are source-glyph index only; no arbitrary screenshot overlay',()=>{
  const result=parseCardIndex({tiles:[{code:104,base:'card-base.png',
    roles:'card-roles.png',carve:true}]},()=>true);
  assert.equal(result.count,1);
  assert.equal(result.items[0].base,'cards/card-base.png');
  assert.equal(result.items[0].roles,'cards/card-roles.png');
  assert.equal(result.items[0].carve,true);
  const denied=parseCardIndex({tiles:[{code:104,base:'../../hack.png'}]});
  assert.equal(denied.ignored,1);
});
test('Mode7 exact key, 256 palette colours and fixed art tiles',()=>{
  const palette=Array(256).fill('123456');
  const raw={version:1,scale:4,tile:256,planes:[{
    id:'earth',key:'0123456789abcdef',tiles:'earth/t{x}_{y}.png',palette
  }]};
  const r=parseMode7Index(raw);
  assert.equal(r.count,1);assert.equal(r.items[0].tiles,'mode7/earth/t{x}_{y}.png');
  assert.equal(parseMode7Index({...raw,planes:[{...raw.planes[0],key:'garbage'}]}).ignored,1);
  assert.equal(parseMode7Index({...raw,planes:[{...raw.planes[0],palette:['abc']}]}).ignored,1);
});
test('all HD categories hold a matched but unready asset only for a bounded number of frames',()=>{
  for(const phase of ['fighter','portrait','field','effect','map','card','mode7']){
    const gate=new PortraitFrameGate({pendingFrames:6,transitionFrames:3});
    gate.didPresent();
    assert.equal(gate.shouldDefer(100,{enabled:true,pending:true,key:phase}),true);
    assert.equal(gate.shouldDefer(101,{enabled:true,pending:true,key:phase}),true);
    assert.equal(gate.shouldDefer(106,{enabled:true,pending:true,key:phase}),false);
    assert.equal(gate.shouldDefer(107,{enabled:true,pending:true,key:phase}),false);
  }
});
test('strict data-path gating retains old Chinese overlay and fallback rules',()=>{
  const bridge=source('crates/ssd-web/src/lib.rs');
  const ui=source('web/app.mjs');
  const importUI=source('web/hd-import-ui.mjs');
  for(const p of ['special','map','card','mode7']){
    assert.match(bridge,new RegExp('self\\.live_'+(p==='special'?'specials':p==='map'?'maps':p==='card'?'cards':'mode7')+'\\.composite\\('));
  }
  const end=bridge.indexOf('self.hd_renderer.paint_native_translation(');
  for(const p of ['specials','maps','cards','mode7']){
    assert.ok(bridge.indexOf('self.live_'+p+'.composite(')<end);
  }
  for(const v of ['liveFighters','livePortraits','liveSpecials','liveMaps','liveCards','liveMode7']){
    assert.match(ui,new RegExp(v+'\\.pulse\\(true\\)'));
  }
  for(const folder of ['hd-special','hd-map','hd-card','hd-mode7']){
    assert.match(importUI,new RegExp('read'+(folder==='hd-special'?'Special':folder==='hd-map'?'Map':folder==='hd-card'?'Card':'Mode7')+'Catalog'));
  }
  assert.match(ui,/!storyHold && !hdHold && !fighterHold && !portraitHold && !specialHold && !mapHold && !cardHold && !mode7Hold/);
  assert.doesNotMatch(ui,/fetch\(.+class[123]\/|fetch\(.+mode7\//);
});
