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
  const report=trace.report({build:'dbz-hd-tw-pwa-v53'});
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


test('records rejected Goku candidate while matched Saibaman still paints HD',()=>{
  const rejected={
    key:'abcdef0123456789',w:49,h:48,
    reason:'palette_mismatch',origin:'primary',
    candidateIds:['battle-goku-003']
  };
  const g=fakeGame({
    probe:{poses:2,keyHits:2,paletteRejected:1,matched:1,
      fallbackAttempted:true,fallbackAccepted:0,rejectedPoses:[rejected]},
    matches:[{id:'battle-saibaman-001',key:'8553f5f31ec52155',
      ready:true,luma:false,width:33,height:32}],
    pixels:489
  });
  const t=new HdFighterDiagnostic();
  t.start(0);
  t.capture(g,options,0);
  g.step();t.capture(g,options,205);
  const report=t.report({build:'dbz-hd-tw-pwa-v53'});
  assert.equal(report.format,'fighter-pose-trace-v4');
  assert.equal(report.summary.sampleCount,2);
  assert.equal(report.summary.partialMatches,2);
  assert.equal(report.summary.samplesWithRejectedPoses,2);
  assert.equal(report.summary.fallbackAttemptedSamples,2);
  assert.equal(report.summary.acceptedRefinedPosesAcrossSamples,0);
  assert.equal(report.summary.rejectionReasons.palette_mismatch,2);
  assert.equal(report.summary.rejectedPoseExamples[0].count,2);
  assert.deepEqual(report.summary.rejectedPoseExamples[0].candidateIds,['battle-goku-003']);
  assert.equal(report.samples[0].probe.rejectedPoses[0].key,rejected.key);
});

test('sanitizes rejected-pose evidence, refuses paths and caps samples',()=>{
  const malicious=[
    {key:'0123456789ABCDEF',w:33,h:32,origin:'primary',reason:'size_mismatch',
      candidateIds:['battle-goku-001','/var/private/photo.png','../secret','battle-saibaman-009']},
    {key:'xyz',w:33,h:32,origin:'primary',reason:'palette_mismatch',candidateIds:['SECRET']},
    {key:'0123456789abcdef',w:0,h:32,origin:'primary',reason:'palette_mismatch',candidateIds:[]},
    {key:'0123456789abcdef',w:33,h:32,origin:'primary',reason:'new_unsafe_action',candidateIds:[]}
  ];
  const g=fakeGame({probe:{poses:1,keyHits:1,rejectedPoses:malicious}});
  const t=new HdFighterDiagnostic();
  t.start(100);t.capture(g,options,100);
  const report=t.report();
  assert.equal(report.samples[0].probe.rejectedPoses.length,1);
  assert.equal(report.samples[0].probe.rejectedPoses[0].key,'0123456789abcdef');
  assert.deepEqual(report.samples[0].probe.rejectedPoses[0].candidateIds,
    ['battle-goku-001','battle-saibaman-009']);
  const payload=JSON.stringify(report);
  assert.doesNotMatch(payload,/\/var\/private|\.\.\/secret|SECRET|new_unsafe_action/);
  assert.ok(payload.length<30_000);
});


test('Goku collision resolution and evidence are sanitized and counted without exposing roster RAM',()=>{
  const g=fakeGame({
    probe:{
      poses:2,keyHits:2,matched:2,
      gokuAmbiguitiesResolved:1,gokuFormEvidence:'normal',
      rejectedPoses:[],fallbackAccepted:0
    },
    matches:[
      {id:'battle-goku-000',key:'732890cac370e639',ready:true,luma:false,width:50,height:48},
      {id:'battle-saibaman-001',key:'0abc5ab8b0bee7b0',ready:true,luma:false,width:34,height:32}
    ],
    pixels:1290
  });
  const t=new HdFighterDiagnostic();t.start(0);
  t.capture(g,options,0);g.step();t.capture(g,options,201);
  const report=t.report({build:'dbz-hd-tw-pwa-v53'});
  assert.equal(report.format,'fighter-pose-trace-v4');
  assert.equal(report.summary.gokuAmbiguitiesResolvedAcrossSamples,2);
  assert.equal(report.summary.gokuFormEvidenceCounts.normal,2);
  assert.equal(report.summary.gokuFormEvidenceCounts.super,0);
  assert.equal(report.summary.rejectionReasons.ambiguous_palette,undefined);
  assert.equal(report.samples[0].probe.gokuFormEvidence,'normal');
  assert.equal(report.samples[0].probe.gokuAmbiguitiesResolved,1);
  assert.deepEqual(report.samples[0].matches.map(x=>x.id),['battle-goku-000','battle-saibaman-001']);
  assert.doesNotMatch(JSON.stringify(report),/partyRecords|wram|characterId|0x0B00/i);
});
test('untrusted Goku form evidence never leaks arbitrary data into exported JSON',()=>{
  const g=fakeGame({
    probe:{poses:1,keyHits:1,gokuAmbiguitiesResolved:0,
      gokuFormEvidence:'../../../secrets'},
    matches:[],pixels:0
  });
  const t=new HdFighterDiagnostic();t.start(0);t.capture(g,options,0);
  const report=t.report();
  assert.equal(report.samples[0].probe.gokuFormEvidence,'unknown');
  assert.equal(report.summary.gokuFormEvidenceCounts.unknown,1);
  assert.doesNotMatch(JSON.stringify(report),/secrets/);
});


test('v53 trace distinguishes raw shared-key assets from palette-qualified candidates',()=>{
  const g=fakeGame({probe:{
    poses:2,keyHits:2,matched:1,gokuFormEvidence:'normal',
    rejectedPoses:[{
      key:'732890cac370e639',w:50,h:48,
      origin:'primary',reason:'ambiguous_palette',
      candidateIds:['battle-goku-000','battle-goku-ssj-000','field-goku-ki-001','battle-piccolo-other'],
      paletteEligibleIds:['battle-goku-000','field-goku-ki-001','battle-piccolo-other'],
      paletteEligibleCount:3
    }]},
    matches:[{id:'battle-saibaman-001',key:'0abc5ab8b0bee7b0',ready:true,width:34,height:32}],
    pixels:489});
  const t=new HdFighterDiagnostic();t.start(0);t.capture(g,options,0);
  const report=t.report({build:'dbz-hd-tw-pwa-v53'});
  assert.equal(report.format,'fighter-pose-trace-v4');
  const rejected=report.samples[0].probe.rejectedPoses[0];
  assert.equal(rejected.paletteEligibleCount,3);
  assert.deepEqual(rejected.paletteEligibleIds,
    ['battle-goku-000','field-goku-ki-001','battle-piccolo-other']);
  assert.equal(rejected.candidateIds.length,4);
  assert.equal(report.summary.rejectedPoseExamples[0].paletteEligibleCount,3);
});
test('v53 diagnostics reject unsafe candidate IDs and unreasonable eligibility counts',()=>{
  const g=fakeGame({probe:{poses:1,keyHits:1,
    rejectedPoses:[{
      key:'732890cac370e639',w:50,h:48,
      origin:'primary',reason:'ambiguous_palette',
      paletteEligibleCount:9999999,
      candidateIds:['battle-goku-000','../../secret'],
      paletteEligibleIds:['battle-goku-000','../secret','/tmp/private']
    }]},matches:[],pixels:0});
  const t=new HdFighterDiagnostic();t.start(0);t.capture(g,options,0);
  const rejected=t.report().samples[0].probe.rejectedPoses[0];
  assert.equal(rejected.paletteEligibleCount,0);
  assert.deepEqual(rejected.candidateIds,['battle-goku-000']);
  assert.deepEqual(rejected.paletteEligibleIds,['battle-goku-000']);
  assert.doesNotMatch(JSON.stringify(t.report()),/secret|private/);
});
test('legacy HD fighter diagnostics are not exposed to players',()=>{
  const app=readFileSync(new URL('../web/app.mjs',import.meta.url),'utf8');
  const html=readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
  const css=readFileSync(new URL('../web/style.css',import.meta.url),'utf8');
  assert.doesNotMatch(html,/fighter-diagnostic/);
  assert.doesNotMatch(app,/fighterDiagnostic|fighter-diagnostic|HdFighterDiagnostic/);
  assert.doesNotMatch(css,/fighter-diagnostic/);
  assert.match(html,/id="cheat-open"/);
  assert.match(html,/id="cheat-done"/);
  assert.ok(html.includes('app.mjs?build=v53&ui=v63'));
  assert.ok(app.includes('if(!running&&!cheatPanelOpen&&!speedNoticeOpen)'));
  assert.ok(app.includes('hdSceneTrace.capture(game,lastPresentation)'));
  const playbar=html.split('<div class="playbar">')[1].split('</div>')[0];
  assert.equal((html.match(/data-bit="/g)||[]).length,10);
  assert.doesNotMatch(playbar,/fighter-diagnostic/);
});
