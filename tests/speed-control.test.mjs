import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {SPEED_LEVELS,SPEED_NOTICE_KEY,nextSpeed,speedFramePlan} from '../web/speed-control.mjs';
import {SnesAudio} from '../web/audio.mjs';

const src=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const STEP=1000/60.0988;

test('single speed button cycles 1x -> 2x -> 3x -> 1x',()=>{
  assert.deepEqual(SPEED_LEVELS,[1,2,3]);
  assert.equal(nextSpeed(1),2);
  assert.equal(nextSpeed(2),3);
  assert.equal(nextSpeed(3),1);
  assert.equal(nextSpeed(999),1);
});

function simulate(rate,{hz=60.0988,averageTickMs=2,seconds=8}={}){
  const frames=Math.round(hz*seconds);
  let accumulatorMs=0,ticks=0,peak=0;
  for(let i=0;i<frames;i++){
    const plan=speedFramePlan({
      gapMs:1000/hz,accumulatorMs,rate,averageTickMs,stepMs:STEP
    });
    accumulatorMs=plan.remainingMs;
    ticks+=plan.ticks;
    peak=Math.max(peak,plan.ticks);
  }
  return {fps:ticks/seconds,peak,accumulatorMs};
}
test('60 Hz display achieves intended 1x 2x 3x when emulator is fast',()=>{
  for(const [rate,target] of [[1,60.0988],[2,120.1976],[3,180.2964]]){
    const out=simulate(rate);
    assert.ok(Math.abs(out.fps-target)<1,JSON.stringify({rate,...out}));
  }
});
test('120 Hz display achieves 1x 2x 3x independently of refresh rate',()=>{
  for(const [rate,target] of [[1,60.0988],[2,120.1976],[3,180.2964]]){
    const out=simulate(rate,{hz:120});
    assert.ok(Math.abs(out.fps-target)<1,JSON.stringify({rate,...out}));
  }
});
test('HD-heavy devices cap work instead of generating an unlimited catchup burst',()=>{
  const result=simulate(3,{averageTickMs:STEP});
  assert.equal(result.peak,1);
  assert.ok(result.fps<=61);
  const stale=speedFramePlan({rate:3,gapMs:9000,accumulatorMs:100000,averageTickMs:0,stepMs:STEP});
  assert.ok(stale.ticks<=3);
  assert.ok(stale.remainingMs<=STEP*3);
  assert.throws(()=>speedFramePlan({rate:1.5,gapMs:16,accumulatorMs:0}),/不支援/);
});
test('fast audio remains enabled at each speed',()=>{
  const fake=new SnesAudio(null),seen=[];
  fake.context={state:'running',currentTime:0};
  fake.enabled=true;
  fake.schedule=frames=>{
    seen.push(fake.playbackRate);
    fake.take(frames,new Float32Array(frames),new Float32Array(frames));
  };
  for(const rate of [1,2,3,1]){
    fake.setPlaybackRate(rate);
    fake.enqueue(new Float32Array(4096));
    assert.equal(fake.pendingFrames,0);
    assert.equal(fake.enabled,true);
  }
  assert.deepEqual(seen,[1,2,3,1]);
});
test('one physical button, portrait and landscape anchors, one-time release modal',()=>{
  const html=src('web/index.html'),css=src('web/style.css'),app=src('web/app.mjs');
  assert.equal([...html.matchAll(/id="speed-toggle"/g)].length,1);
  assert.match(html,/id="speed-portrait-anchor"/);
  assert.match(html,/id="speed-landscape-anchor"/);
  assert.match(html,/id="speed-update-panel"/);
  assert.match(html,/id="speed-update-close"/);
  assert.match(html,/新增 10 格存檔、讀取及備份功能/);
  assert.match(html,/HD 美術包快速啟動，不再每次完整驗證/);
  assert.match(html,/新增一鍵倍速：1× → 2× → 3×/);
  assert.doesNotMatch(html,/1\.5×/);
  assert.match(app,/landscapeSpeedPlacement\.matches/);
  assert.match(app,/anchor\.appendChild\(\$\('speed-toggle'\)\)/);
  assert.match(css,/\.playing #game \.speed-portrait-anchor/);
  assert.match(css,/\.playing #game \.speed-landscape-anchor/);
  assert.match(app,/const plan=speedFramePlan\(/);
  assert.match(app,/sound\.setPlaybackRate\(gameSpeed\)/);
  assert.match(app,/localStorage\.getItem\(SPEED_NOTICE_KEY\)/);
  assert.match(app,/localStorage\.setItem\(SPEED_NOTICE_KEY,'1'\)/);
  assert.match(app,/if\(showSpeedUpdateNoticeOnce\(\)\)running=false/);
  const switchHandler=app.slice(app.indexOf("$('speed-toggle').addEventListener"),app.indexOf('function showSpeedUpdateNoticeOnce'));
  assert.ok(switchHandler.includes('updateSpeedButton()'));
  assert.doesNotMatch(switchHandler,/gameToast\(/,'speed switching must not paint a toast across the speed button');
  assert.match(css,/speed-toggle\[data-speed="3"\]/);
  assert.equal(SPEED_NOTICE_KEY,'dbz-ssd-update-2026-11-22-v56b-seen');
  assert.match(html,/app\.mjs\?build=v66&ui=v66/);
  assert.match(html,/style\.css\?build=v50&ui=v64/);
  for(const bit of [0,1,2,3,4,5,6,7,8,9])
    assert.equal([...html.matchAll(new RegExp('data-bit="'+bit+'"','g'))].length,1);
});
