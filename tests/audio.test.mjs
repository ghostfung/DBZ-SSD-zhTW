import test from 'node:test';
import assert from 'node:assert/strict';
import { SnesAudio } from '../web/audio.mjs';

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
  createBuffer(channels, length, rate) {
    assert.equal(channels, 2);
    assert.equal(rate, this.sampleRate);
    const data = [new Float32Array(length), new Float32Array(length)];
    this.latest = data;
    return { getChannelData: n => data[n] };
  }
  createBufferSource() {
    return { connect() {}, start: at => { this.started.push(at); } };
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
