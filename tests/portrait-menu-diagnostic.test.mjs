import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {makePortraitMenuReport} from '../web/portrait-menu-diagnostic.mjs';

const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('Namek trace explains portrait flicker with no ROM or save-state payload',()=>{
  const snapshot={build:{client:'test',matched:true},recentSceneTrace:{
    samples:[
      {frame:100,presentation:{displayedFrame:100,held:false,reasons:[]},portrait:{
        matched:2,ready:2,pixels:600,items:[{id:'piccolo',ready:true,x:1,y:2}],
        probe:{candidate:3,matched:2,ready:2,weak:1,paletteRejected:0,sourceWriteRejected:0,reason:'ok'}
      }},
      {frame:106,presentation:{displayedFrame:106,held:false,reasons:[]},portrait:{
        matched:0,ready:0,pixels:0,items:[],
        probe:{candidate:2,matched:0,ready:0,weak:2,paletteRejected:1,sourceWriteRejected:1,reason:'weak'}
      }},
      {frame:112,presentation:{displayedFrame:100,held:true,reasons:['portrait']},portrait:{
        matched:1,ready:1,pixels:0,items:[{id:'goku',ready:true,x:3,y:4}],
        probe:{candidate:1,matched:1,ready:1,weak:0,paletteRejected:0,sourceWriteRejected:0,reason:'composite'}
      }}
    ]
  }};
  const report=makePortraitMenuReport(snapshot);
  assert.equal(report.summary.samples,3);
  assert.equal(report.summary.noHdPixels,2);
  assert.equal(report.summary.noMatches,1);
  assert.equal(report.summary.readyButNoPixels,1);
  assert.equal(report.summary.framesWithWeakCandidates,2);
  assert.equal(report.summary.framesWithChangedSourceRejected,1);
  assert.deepEqual(report.frames[2].holdReasons,['portrait']);
  assert.equal(report.frames[0].portrait.entries[0].id,'piccolo');
  assert.equal(report.build.coreMatched,true);
  assert.ok(!JSON.stringify(report).includes('byteArray'));
});

test('trace is bounded, sanitizes fields and includes no raw assets',()=>{
  const samples=Array.from({length:260},(_,i)=>({
    frame:i,presentation:{reasons:['portrait']},
    portrait:{matched:0,ready:0,pixels:0,items:[{id:'X'.repeat(900),x:1,y:1}],
      probe:{reason:'X'.repeat(900),weak:0}}
  }));
  const report=makePortraitMenuReport({recentSceneTrace:{samples}});
  assert.equal(report.frames.length,120);
  assert.equal(report.frames[0].frame,140);
  assert.equal(report.frames[0].portrait.entries[0].id.length,90);
  assert.equal(report.frames[0].portrait.probe.reason.length,90);
  const js=source('web/portrait-menu-diagnostic.mjs');
  assert.match(js,/hd-coverage-runtime-snapshot/);
  assert.match(js,/report.frames.length<10/);
  assert.match(js,/JSON.stringify\(report,null,2\)/);
  assert.doesNotMatch(js,/save_state\(|frame_rgba\(|readGameFile\(/);
});

test('trace export is reachable only through the existing game menu',()=>{
  const html=source('web/index.html');
  assert.equal((html.match(/id="portrait-menu-export"/g)||[]).length,1);
  assert.match(html,/portrait-menu-diagnostic\.mjs\?build=v57trace1/);
  const bar=html.split('<div class="playbar">')[1].split('</div>')[0];
  assert.doesNotMatch(bar,/portrait-menu-export/);
  assert.match(html,/id="save-slots-panel"/);
  assert.match(html,/id="cheat-open"/);
});
