import test from 'node:test';
import assert from 'node:assert/strict';
import { SnesAudio } from '../web/audio.mjs';
import { readFileSync } from 'node:fs';

class FakeAudioContext {
  constructor() {
    this.sampleRate = 48000;
    this.currentTime = 1;
    this.state = 'suspended';
    this.started = [];
    this.destination = {};
  }
  async resume() { this.state = 'running'; }
  async suspend() { this.state = 'suspended'; }
  async close() { this.state = 'closed'; }
  createBuffer(channels, length, rate) {
    assert.equal(channels, 2);
    assert.equal(rate, this.sampleRate);
    const data = [new Float32Array(length), new Float32Array(length)];
    this.latest = data;
    return { getChannelData: n => data[n] };
  }
  createBufferSource() {
    const node={playbackRate:{value:1},connect(){},disconnect(){},stop(){this.stopped=true},start:at=>{this.started.push(at);}};
    (this.nodes??=[]).push(node);return node;
  }
}

test('Safari gesture unlock and stereo PCM scheduling', async () => {
  const audio = new SnesAudio(FakeAudioContext);
  assert.equal(audio.sampleRate, 48000);
  await audio.enable();
  assert.equal(audio.state, 'running');
  const block = new Float32Array(4096);
  for (let i = 0; i < 2048; i++) {
    block[2 * i] = 0.25;
    block[2 * i + 1] = -0.25;
  }
  audio.enqueue(block);
  assert.equal(audio.context.started.length, 1);
  assert.equal(audio.context.started[0], 1.075);
  assert.equal(audio.context.latest[0][0], 0.25);
  assert.equal(audio.context.latest[1][2047], -0.25);
  assert.equal(audio.pendingFrames, 0);
  await audio.mute();
  assert.equal(audio.enabled, false);
  assert.equal(audio.context.state, 'suspended');
});

test('PCM samples can span multiple emulated frames', async () => {
  const audio = new SnesAudio(FakeAudioContext);
  await audio.enable();
  for (let i = 0; i < 3; i++) audio.enqueue(new Float32Array(1600));
  assert.equal(audio.context.started.length, 1);
  assert.equal(audio.pendingFrames, 352);
  audio.reset();
  assert.equal(audio.pendingFrames, 0);
});

test('muted output is discarded; malformed stereo rejected', async () => {
  const audio = new SnesAudio(FakeAudioContext);
  audio.enqueue(new Float32Array(4096));
  assert.equal(audio.pendingFrames, 0);
  await audio.enable();
  assert.throws(() => audio.enqueue(new Float32Array(3)), /雙聲道/);
});

test('file picker interruption restores suspended AudioContext without unmuting by force',async()=>{
  const audio=new SnesAudio(FakeAudioContext);
  assert.equal(await audio.resumeAfterInterruption(),false);
  await audio.enable();
  assert.equal(audio.enabled,true);
  await audio.context.suspend();
  assert.equal(audio.state,'suspended');
  assert.equal(await audio.resumeAfterInterruption(),true);
  assert.equal(audio.state,'running');
  await audio.mute();
  assert.equal(await audio.resumeAfterInterruption(),false);
  assert.equal(audio.enabled,false);
});

test('iPhone gesture reconnect recreates a silent-but-running context and restarts PCM',async()=>{
  const audio=new SnesAudio(FakeAudioContext);
  await audio.enable();
  const old=audio.context;
  audio.enqueue(new Float32Array(4096));
  assert.equal(audio.context.started.length,1);
  // Safari can report "running" but output silence after switching apps.
  assert.equal(audio.state,'running');
  const rate=await audio.reconnectFromGesture();
  assert.equal(rate,48000);
  assert.notEqual(audio.context,old);
  assert.equal(old.state,'closed');
  assert.equal(audio.context.state,'running');
  assert.equal(audio.enabled,true);
  assert.equal(audio.pendingFrames,0);
  assert.equal(audio.context.started.length,1,'new context was primed inside tap');
  audio.enqueue(new Float32Array(4096));
  assert.equal(audio.context.started.length,2,'subsequent emulator PCM reaches new context');
});

test('audio reconnect remains possible after an interrupted context',async()=>{
  const audio=new SnesAudio(FakeAudioContext);
  await audio.enable();
  const old=audio.context;
  old.state='interrupted';
  assert.equal(await audio.reconnectFromGesture(),48000);
  assert.equal(audio.state,'running');
  assert.equal(old.state,'closed');
});

test('manual audio recovery stays available after visibilitychange even when context reports running',()=>{
  const app=readFileSync(new URL('../web/app.mjs',import.meta.url),'utf8');
  assert.match(app,/offerManualSoundRecovery=Boolean\(sound\.enabled\)/);
  assert.match(app,/await sound\.reconnectFromGesture\(\)/);
  assert.match(app,/offerManualSoundRecovery\?'重新連接聲音':'恢復聲音'/);
  assert.match(app,/if\(game\)game\.set_audio_rate\(rate\)/);
});

test('1x, 2x and 3x PCM play without mute; old sources are stopped on rate changes',async()=>{
  const audio=new SnesAudio(FakeAudioContext);
  await audio.enable();
  const ctx=audio.context;
  for(const rate of [1,2,3,1]){
    audio.setPlaybackRate(rate);
    audio.enqueue(new Float32Array(8192).fill(.1));
    const [first,second]=ctx.nodes.slice(-2);
    assert.equal(first.playbackRate.value,rate);
    assert.equal(second.playbackRate.value,rate);
    assert.ok(Math.abs((ctx.started.at(-1)-ctx.started.at(-2))-2048/(48000*rate))<1e-9);
    assert.equal(audio.context,ctx);
    assert.equal(audio.enabled,true);
    assert.ok(ctx.nodes.slice(0,-2).every(node=>node.stopped));
  }
});
