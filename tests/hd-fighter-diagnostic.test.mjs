import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {
  HdFighterDiagnostic, classifyFighterSample, FIGHTER_TRACE_VERSION
} from '../web/hd-fighter-diagnostic.mjs';

function fakeGame({probe={},matches=[],pixels=0,active=true}={}){
  let frame=64000;
  return {
    step:()=>{frame++;},
    hd_active:()=>active,
    frame_count:()=>frame,
    fighter_probe:()=>JSON.stringify(probe),
    fighter_matches:()=>JSON.stringify(matches),
    hd_fighter_pixels:()=>pixels
  };
}
const options={catalogReady:true,catalogEntries:5000,fighterEnabled:true,loading:0,failed:0};

test('bounded sampling records only at 5 Hz and auto stops by duration',()=>{
  const game=fakeGame({probe:{poses:2,keyHits:0,unindexedKeys:['abcdef0123456789/32x48']}});
  const trace=new HdFighterDiagnostic({sampleMs:200,maxMs:1000,maxSamples:9});
  trace.start(0);
  assert.equal(trace.capture(game,options,0),true);
  game.step();
  assert.equal(trace.capture(game,options,125),false);
  assert.equal(trace.capture(game,options,200),true);
  game.step();
  assert.equal(trace.capture(game,options,400),true);
  game.step();
  assert.equal(trace.capture(game,options,1100),false);
  assert.equal(trace.recording,false);
  const report=trace.report({build:'dbz-hd-tw-pwa-v50'});
  assert.equal(report.summary.sampleCount,3);
  assert.equal(report.summary.categoryCounts.source_key_unmatched,3);
  assert.equal(report.summary.unindexedSourceKeys[0].key,'abcdef0123456789/32x48');
  assert.equal(report.format,FIGHTER_TRACE_VERSION);
  assert.equal(report.endReason,'time_or_sample_limit');
});

test('categorises matched HD, not-ready art, drawn-zero, palette rejection and absent catalogue',()=>{
  const base={hdActive:true,catalogReady:true,fighterEnabled:true,
    probe:{poses:2,keyHits:1,fallbackPoses:0,fallbackKeyHits:0},
    matches:[],replacedPixels:0};
  assert.equal(classifyFighterSample({...base,probe:{poses:2,keyHits:0,fallbackPoses:0,fallbackKeyHits:0}}),'source_key_unmatched');
  assert.equal(classifyFighterSample({...base}),'palette_size_or_ambiguity');
  assert.equal(classifyFighterSample({...base,matches:[{ready:false}]}),'matched_art_not_ready');
  assert.equal(classifyFighterSample({...base,matches:[{ready:true}]}),'ready_but_not_painted');
  assert.equal(classifyFighterSample({...base,matches:[{ready:true}],replacedPixels:120}),'hd_pixels_replaced');
  assert.equal(classifyFighterSample({...base,hdActive:false}),'hd_inactive');
  assert.equal(classifyFighterSample({...base,catalogReady:false}),'catalog_missing');
  assert.equal(classifyFighterSample({...base,fighterEnabled:false}),'fighter_disabled');
  assert.equal(classifyFighterSample({...base,probe:null}),'probe_unavailable');
  assert.equal(classifyFighterSample({...base,probe:{poses:0,fallbackPoses:0,keyHits:0}}),'no_fighter_pose');
});

test('report sanitises oversized or malformed index items and never embeds private game data',()=>{
  const game=fakeGame({
    probe:{poses:2,keyHits:1,paletteRejected:0,
      unindexedKeys:['0123456789abcdef/32x48','../../private','other/no',23],
      junk:{ROM:'SECRET'}},
    matches:[{
      id:'goku-stance'.repeat(30),key:'ABCDEF0123456789',
      ready:true,luma:false,width:32,height:48,
      art:'/Users/private/HD/goku.png',
      pixels:[1,2,3],saveState:'PRIVATE'
    }],
    pixels:389
  });
  const trace=new HdFighterDiagnostic();
  trace.start(500);trace.capture(game,options,700);trace.stop('manual',901);
  const r=trace.report({build:'v50'});
  assert.equal(r.summary.categoryCounts.hd_pixels_replaced,1);
  assert.equal(r.samples[0].matches[0].key,'abcdef0123456789');
  assert.equal(r.samples[0].matches[0].id.length,90);
  assert.equal(r.summary.unindexedSourceKeys.length,1);
  assert.equal(r.samples[0].matches[0].ready,true);
  const json=JSON.stringify(r);
  for(const forbidden of ['/Users/private','SECRET','PRIVATE','ROM:','saveState','rawPixels']){
    assert.equal(json.includes(forbidden),false,forbidden);
  }
  assert.doesNotMatch(json,/HD\/goku\.png/);
  assert.equal(r.summary.sampleCount,1);
});

test('no active recording means no extra WASM diagnostics are queried',()=>{
  let calls=0;
  const game={
    frame_count:()=>1,
    fighter_probe:()=>{calls++;return '{}';},
    fighter_matches:()=>{calls++;return '[]';}
  };
  const trace=new HdFighterDiagnostic();
  assert.equal(trace.capture(game,options,1),false);
  assert.equal(calls,0);
});

test('sample cap prevents unbounded capture and invalid bounds are rejected',()=>{
  assert.throws(()=>new HdFighterDiagnostic({sampleMs:1}),/上限/);
  assert.throws(()=>new HdFighterDiagnostic({maxSamples:301}),/上限/);
  const game=fakeGame({probe:{poses:0},matches:[]});
  const trace=new HdFighterDiagnostic({sampleMs:150,maxMs:90000,maxSamples:2});
  trace.start(0);trace.capture(game,options,0);game.step();trace.capture(game,options,150);
  assert.equal(trace.recording,false);
  assert.equal(trace.samples.length,2);
  trace.start(200);assert.equal(trace.samples.length,0);
});

test('test-only diagnostic UI is reachable from existing game menu without adding a new gamepad key',()=>{
  const app=readFileSync(new URL('../web/app.mjs',import.meta.url),'utf8');
  const html=readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
  const css=readFileSync(new URL('../web/style.css',import.meta.url),'utf8');
  assert.match(html,/id="fighter-diagnostic-open"/);
  assert.match(html,/id="fighter-diagnostic-panel"/);
  assert.match(html,/id="fighter-diagnostic-start"/);
  assert.match(html,/id="fighter-diagnostic-export"/);
  assert.match(html,/app\.mjs\?build=v50&ui=v56/);
  assert.match(app,/fighterDiagnostic\.capture\(game/);
  assert.match(app,/fighterDiagnostic\.recording/);
  assert.match(app,/const fighterDiagnostic = new HdFighterDiagnostic/);
  assert.match(app,/if\(!running&&!cheatPanelOpen&&!fighterDiagnosticPanelOpen\)/);
  assert.match(app,/link\.download='dbz-hd-fighter-diagnostic-v50\.json'/);
  assert.match(css,/\.fighter-diagnostic-entry\{/);
  const playbar=html.split('<div class="playbar">')[1].split('</div>')[0];
  assert.doesNotMatch(playbar,/fighter-diagnostic/);
  assert.equal((html.match(/data-bit="/g)||[]).length,10);
});
