// Browser-side stereo PCM playback. The emulator feeds interleaved f32 samples
// at AudioContext.sampleRate, and user gestures explicitly unlock Safari audio.
const CHUNK_FRAMES = 2048;
const START_LEAD_SECONDS = 0.075;
const MAX_LEAD_SECONDS = 0.28;

export class SnesAudio {
  constructor(Context = globalThis.AudioContext || globalThis.webkitAudioContext) {
    this.Context = Context;
    this.context = null;
    this.enabled = false;
    this.pending = [];
    this.pendingFrames = 0;
    this.offsetFrames = 0;
    this.nextStart = 0;
  }

  get sampleRate() { return this.context?.sampleRate || 48000; }
  get state() { return this.context?.state || 'uninitialized'; }

  async enable() {
    if (!this.Context) throw new Error('這個瀏覽器不支援 Web Audio API');
    if (!this.context || this.context.state === 'closed') this.context = new this.Context();
    // Must originate from a user click to unlock audio on iPhone Safari.
    await this.context.resume();
    if (this.context.state !== 'running') throw new Error('Safari 尚未允許播放聲音，請再按一次「啟用聲音」');
    this.enabled = true;
    this.reset();
    return this.sampleRate;
  }

  async resumeAfterInterruption() {
    // iOS can interrupt an AudioContext on app switch or file selection.
    // Resume is best-effort; 'running' alone does NOT guarantee audible output.
    if (!this.enabled || !this.context || this.context.state === 'closed') return false;
    const context = this.context;
    if (context.state !== 'running') await context.resume();
    if (context !== this.context || context.state !== 'running') return false;
    this.reset();
    return true;
  }

  async reconnectFromGesture() {
    // Called synchronously from an actual user tap. iOS Safari sometimes
    // reports 'running' on a silent AudioContext after backgrounding; simply
    // calling resume() again will not recover that output route.
    if (!this.Context) throw new Error('這個瀏覽器不支援 Web Audio API');
    const old = this.context;
    this.enabled = false;
    this.reset();
    if (old && old.state !== 'closed' && typeof old.close === 'function') {
      try {
        const closing = old.close(); // Initiate before creating a replacement.
        closing?.catch?.(() => {});
      } catch { /* Replacement remains available if closing fails. */ }
    }
    const fresh = new this.Context();
    this.context = fresh;
    // Start a tiny silent buffer inside the click handler to activate WebKit's
    // audio rendering graph. PCM is still supplied only by the emulator.
    const warmup = fresh.createBuffer(2, 128, fresh.sampleRate);
    const source = fresh.createBufferSource();
    source.buffer = warmup;
    source.connect(fresh.destination);
    source.start();
    // Some WebKit interruptions leave resume() permanently pending. Bound
    // the manual attempt so the button is re-enabled for another tap.
    let deadline;
    try {
      await Promise.race([
        fresh.resume(),
        new Promise((_, reject) => {
          deadline = setTimeout(() => reject(new Error(
            'iOS 音訊啟動逾時；可再次點擊重新連接聲音'
          )), 3500);
        }),
      ]);
    } finally {
      clearTimeout(deadline);
    }
    if (fresh !== this.context || fresh.state !== 'running') {
      throw new Error('Safari 未恢復音訊輸出，請再點一次重新連接聲音');
    }
    this.enabled = true;
    this.reset();
    return this.sampleRate;
  }

  async mute() {
    this.enabled = false;
    this.reset();
    if (this.context?.state === 'running') await this.context.suspend();
  }

  reset() {
    this.pending = [];
    this.pendingFrames = 0;
    this.offsetFrames = 0;
    this.nextStart = 0;
  }

  // Accept interleaved float32 samples: L0, R0, L1, R1, ...
  enqueue(samples) {
    if (!this.enabled || this.context?.state !== 'running' || !samples?.length) return;
    if (samples.length % 2) throw new Error('模擬器提供的 PCM 樣本數不是雙聲道');
    this.pending.push(samples);
    this.pendingFrames += samples.length / 2;
    while (this.pendingFrames >= CHUNK_FRAMES) {
      // Drop stale samples instead of letting browser audio latency grow unchecked.
      if (this.nextStart > this.context.currentTime + MAX_LEAD_SECONDS) this.drop(CHUNK_FRAMES);
      else this.schedule(CHUNK_FRAMES);
    }
  }

  take(frames, left, right) {
    let written = 0;
    while (written < frames) {
      const head = this.pending[0];
      const remaining = head.length / 2 - this.offsetFrames;
      const n = Math.min(remaining, frames - written);
      for (let i = 0; i < n; i++) {
        left[written + i] = head[2 * (this.offsetFrames + i)];
        right[written + i] = head[2 * (this.offsetFrames + i) + 1];
      }
      written += n;
      this.offsetFrames += n;
      if (this.offsetFrames >= head.length / 2) {
        this.pending.shift();
        this.offsetFrames = 0;
      }
    }
    this.pendingFrames -= frames;
  }

  drop(frames) {
    this.take(frames, new Float32Array(frames), new Float32Array(frames));
  }

  schedule(frames) {
    const ctx = this.context;
    const audioBuffer = ctx.createBuffer(2, frames, ctx.sampleRate);
    this.take(frames, audioBuffer.getChannelData(0), audioBuffer.getChannelData(1));
    const node = ctx.createBufferSource();
    node.buffer = audioBuffer;
    node.connect(ctx.destination);
    if (!this.nextStart || this.nextStart < ctx.currentTime + 0.022) {
      this.nextStart = ctx.currentTime + START_LEAD_SECONDS;
    }
    node.start(this.nextStart);
    this.nextStart += frames / ctx.sampleRate;
  }
}
