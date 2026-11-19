import {inspectRom} from './rom-check.mjs';
import {SnesAudio} from './audio.mjs';
import {LiveHdArt} from './hd-live.mjs';
import {StoryFrameGate} from './story-frame-gate.mjs';
import {HdFrameGate} from './hd-frame-gate.mjs';
import {LiveHdFighters} from './hd-fighter-live.mjs';
import {LiveHdPortraits} from './hd-portrait-live.mjs';
const $ = id => document.getElementById(id);
let moduleReady = null, game = null, held = 0, running = false, raf = 0;
let lastTick = 0, accumulator = 0, countWindow = 0, countFrames = 0;
let lastHdPoll = 0, lastDiagPoll = 0, translationKey = '';
let currentDialogue = null, embedded = false, hdEnabled = false;
const STEP = 1000/60.0988;
const sound = new SnesAudio();
const liveHd = new LiveHdArt(message => { $('hd-live-info').textContent = message; });
const liveFighters = new LiveHdFighters(message => { $('hd-fighter-info').textContent = message; });
const livePortraits = new LiveHdPortraits(message => { $('hd-portrait-info').textContent = message; });
const storyFrameGate = new StoryFrameGate(10);
const hdFrameGate = new HdFrameGate();

let translationDiag = [], lastDiagSignature = '';
function sampleTranslationDiagnostics() {
  if (!game || !hdEnabled || typeof game.translation_diagnostics !== 'function') return;
  try {
    const item = JSON.parse(game.translation_diagnostics());
    item.presentation_holding = storyFrameGate.isHolding;
    item.presentation_held_frames = storyFrameGate.heldFramesTotal;
    item.hd_presentation_holding = hdFrameGate.isHolding;
    item.hd_presentation_held_frames = hdFrameGate.heldFramesTotal;
    item.hd_fighter_pixels = game.hd_fighter_pixels?.() ?? 0;
    item.hd_portrait_pixels = game.hd_portrait_pixels?.() ?? 0;
    // Only log changes (page key, detected glyph positions or fallback), rather
    // than holding 60 large JSON objects per second in mobile Safari memory.
    const signature = JSON.stringify([item.start_key,item.end_key,item.late_page,item.visible_page_fallback,item.visible_page_score,item.untranslated_visible_key,item.changed_glyph_samples,item.selected_expected_english,item.story_translation_pending,item.presentation_holding,item.presentation_held_frames,item.hd_presentation_holding,item.hd_presentation_held_frames,item.glyphs]);
    if (signature !== lastDiagSignature) {
      lastDiagSignature = signature;
      translationDiag.push(item);
      if (translationDiag.length > 120) translationDiag.shift();
    }
  } catch (error) {
    console.warn('繁中狀態診斷無法讀取', error);
  }
}

document.addEventListener('hd-catalog-ready', event => {
  liveHd.setCatalog(event.detail.catalog, event.detail.assets);
  hdFrameGate.reset();
});
document.addEventListener('hd-fighter-catalog-ready', event => {
  liveFighters.setCatalog(event.detail.catalog, event.detail.assets);
});
document.addEventListener('hd-portrait-catalog-ready', event => {
  livePortraits.setCatalog(event.detail.catalog, event.detail.assets);
});

function soundUi(message) {
  $('sound').textContent = sound.enabled && sound.state === 'running' ? '靜音' : '啟用聲音';
  $('sound-status').textContent = message || (sound.enabled && sound.state === 'running' ? `聲音已啟用 · ${sound.sampleRate} Hz` : '未啟用聲音');
}

function unlockAudio() {
  // Calling resume() before the first await is required for Safari's user-gesture policy.
  const promise = sound.enable();
  promise.then(rate => {
    if (game) game.set_audio_rate(rate);
    soundUi(`聲音已啟用 · ${rate} Hz`);
  }).catch(err => soundUi(`無法播放聲音：${err.message}`));
  return promise;
}

const setStatus = msg => { $('status').textContent = msg; };
const romInput = $('rom'), ipsInput = $('ips');

romInput.addEventListener('change', async () => {
  if (!romInput.files?.[0]) return;
  const check = inspectRom(await romInput.files[0].arrayBuffer());
  setStatus(`${check.label}\n${check.crc ? `CRC32: ${check.crc}` : ''}`);
  $('start').disabled = check.kind === 'invalid' || check.kind === 'unknown';
});

async function ensureEngine() {
  if (moduleReady) return moduleReady;
  try {
    const mod = await import('./pkg/ssd_web.js');
    await mod.default();
    moduleReady = mod;
    return mod;
  } catch (error) {
    throw Error(`瀏覽器頁面已就緒，但尚未成功編譯 Rust WebAssembly 遊戲核心。\n${error.message}`);
  }
}

$('start').addEventListener('click', async () => {
  // Begin unlocking Safari audio synchronously on this tap (before loading files/WASM).
  unlockAudio();
  try {
    $('start').disabled = true;
    const rom = new Uint8Array(await romInput.files[0].arrayBuffer());
    const ips = ipsInput.files?.[0] ? new Uint8Array(await ipsInput.files[0].arrayBuffer()) : new Uint8Array(0);
    const { WebGame } = await ensureEngine();
    game = new WebGame(rom, ips);
    // Translation remains on-device: the TSV loads from this site, never from the user's ROM.
    const translationResponse = await fetch('./zh-tw.tsv');
    if (!translationResponse.ok) throw Error('繁體中文翻譯包未部署（HTTP '+translationResponse.status+'），請檢查 GitHub Actions。');
    // Do not edit Ming Liu's licensed source TSV. Small, independently
    // reviewed page alignments are appended only in this iPhone prototype.
    const alignResponse = await fetch('./zh-tw-page-alignments.tsv');
    if (!alignResponse.ok) throw Error('繁體中文分頁對齊檔未部署（HTTP '+alignResponse.status+'）。');
    const primary = await translationResponse.text();
    const alignments = await alignResponse.text();
    const translatedRows = game.load_translation(primary.trimEnd() + '\n' + alignments.trim());
    $('translation-count').textContent = '已載入 ' + translatedRows + ' 筆繁體中文譯文';
    translationKey = '';
    currentDialogue = null;
    embedded = false;
    translationDiag = []; lastDiagSignature = '';
    storyFrameGate.reset();
    hdFrameGate.reset();
    game.set_hd_mode(hdEnabled);
    game.set_fighter_hd_enabled($('fighter-hd-toggle').checked);
    game.set_portrait_hd_enabled($('portrait-hd-toggle').checked);
    liveHd.attach(game);
    liveFighters.attach(game);
    livePortraits.attach(game);
    if (sound.enabled && sound.state === 'running') game.set_audio_rate(sound.sampleRate);
    soundUi();
    $('setup').hidden = true;
    $('game').hidden = false;
    held = 0; running = true; lastTick = 0; accumulator = 0; countWindow = 0; countFrames = 0;
    raf = requestAnimationFrame(requestFrame);
  } catch (error) {
    $('start').disabled = false;
    setStatus(String(error));
  }
});

const ctx = $('screen').getContext('2d', {alpha:false});
function drawFrame() {
  const w=game.width(), h=game.height();
  const canvas=$('screen');
  if (w===0 || h===0) return;
  if (canvas.width!==w || canvas.height!==h) {
    canvas.width=w; canvas.height=h;
    storyFrameGate.reset();
    hdFrameGate.reset();
  }
  // A fast advance can expose the next page's native English glyphs before
  // the Rust stateful Chinese renderer has filled that frame. Hold only the
  // *last presented picture* for up to ten simulated frames, not the game:
  // inputs, sound and emulated time continue without interruption.
  const pixels=game.hd_art_pixels();
  const enhanced=game.hd_active();
  const storyHold=storyFrameGate.shouldDefer(
    game.frame_count(), game.story_translation_pending(), enhanced
  );
  // A new PPU background can be matched a few frames before its local 2x
  // picture finishes decoding. Keep the last HD picture temporarily instead
  // of painting the raw SNES background. Never hold indefinitely.
  let pendingArt=false;
  if (enhanced && hdEnabled && liveHd.assets && pixels===0) {
    try {
      pendingArt=JSON.parse(game.background_matches()).some(m=>!m.ready);
    } catch { /* Corrupt/absent catalogue: fail open to the original frame. */ }
  }
  const hdHold=hdFrameGate.shouldDefer(game.frame_count(),{
    enabled:enhanced && hdEnabled && Boolean(liveHd.assets),
    pixels,pending:pendingArt
  });
  if (!storyHold && !hdHold) {
    const rgba=game.frame_rgba();
    if (rgba.length!==w*h*4) throw Error('核心輸出的 RGBA 畫面大小錯誤');
    ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba),w,h),0,0);
    storyFrameGate.didPresent();
    hdFrameGate.didPresent(pixels);
  }
  const native = game.native_translation_active();
  if (native) {
    // The Rust renderer already paints Chinese in the final RGBA pixels.
    // Do not run legacy Canvas guesses or draw overlapping English/Chinese.
    embedded = true;
    $('translation-card').hidden = true;
  } else {
    // Classic/unsupported mode has no PPU-driven Chinese text renderer yet.
    // Keep a separate caption only; never erase menu/status pixels by colour.
    embedded = false;
    $('translation-card').hidden = !currentDialogue;
  }
  canvas.style.imageRendering=game.hd_active()?'auto':'pixelated';
}
function requestFrame(now) {
  if (!running) return;
  try {
    if (!lastTick) lastTick = now;
    // Render and simulation clocks are independent, so 120 Hz displays don't speed up gameplay.
    accumulator += Math.min(now-lastTick, STEP*3);
    let n = 0;
    while(accumulator >= STEP && n < 3) { game.tick(held); countFrames++; n++; accumulator -= STEP; }
    if (n) {
      sound.enqueue(game.drain_audio());
      // Synchronise the message with the PPU from the same emulation frame.
      // The former 80 ms timer displayed bare English before Chinese.
      updateTranslation();
    }
    // Accelerate background catalogue polling only while a new HD layer is
    // missing; 90 ms avoids waiting for the old 450 ms idle interval.
    const hdPending = hdEnabled && liveHd.assets && game.hd_art_pixels()===0;
    // Animated OAM poses may last only a few frames, so poll their catalogue
    // at 90 ms while a local class2 ZIP is attached.
    const interval = hdPending || (hdEnabled && (liveFighters.assets || livePortraits.assets)) ? 90 : 450;
    if (now - lastHdPoll >= interval) {
      lastHdPoll = now;
      liveHd.pulse();
      liveFighters.pulse();
      livePortraits.pulse();
    }
    drawFrame();
    if (now - lastDiagPoll >= 450) {
      lastDiagPoll = now;
      sampleTranslationDiagnostics();
    }
    $('frames').textContent=`${game.frame_count()} 幀`;
    if(!countWindow) countWindow=now;
    if(now-countWindow>=1000){
      $('fps').textContent=`約 ${(countFrames*1000/(now-countWindow)).toFixed(1)} 模擬幀/秒`;
      $('hd-info').textContent=hdEnabled
        ? (game.hd_active() ? `HD 2x：${game.width()}×${game.height()}` : `HD 已回退 Classic：${game.hd_warning()}`)
        : 'Classic 原版畫面';
      countWindow=now;countFrames=0;
    }
    lastTick=now;
    raf=requestAnimationFrame(requestFrame);
  } catch(e) { running=false; window.alert(`遊戲執行失敗：${e}`); }
}

function updateTranslation() {
  const box = $('translation-card');
  if (!game || game.native_translation_active()) {
    currentDialogue = null;
    box.hidden = true;
    translationKey = '';
    return;
  }
  // Caption-only fallback: do not simulate the native battle/window renderer.
  const serialized = game.current_dialogue();
  const msg = serialized ? JSON.parse(serialized) : null;
  currentDialogue = msg?.text ? msg : null;
  if (!currentDialogue) {
    box.hidden = true;
    translationKey = '';
    return;
  }
  const signature = msg.key + '|' + msg.text;
  if (translationKey !== signature) {
    $('translation-text').textContent = msg.text;
    $('translation-key').textContent = msg.key;
    translationKey = signature;
  }
  box.hidden = !$('inframe').checked;
}
$('inframe').addEventListener('change', () => {
  // Native translation always stays in-frame in HD mode; this checkbox now
  // controls only whether a Classic-mode caption is displayed.
  embedded = false;
  if (game) updateTranslation();
});
$('fighter-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_fighter_hd_enabled($('fighter-hd-toggle').checked);
});
$('portrait-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_portrait_hd_enabled($('portrait-hd-toggle').checked);
});
$('hd-toggle').addEventListener('click', () => {
  hdEnabled = !hdEnabled;
  $('hd-toggle').textContent = hdEnabled ? '關閉 HD 2x' : '啟用 HD 2x';
  $('hd-info').textContent = hdEnabled ? '正在初始化 HD 渲染…' : 'Classic 原版畫面';
  if (game) game.set_hd_mode(hdEnabled);
  storyFrameGate.reset();
  hdFrameGate.reset();
  liveHd.pulse();
  liveFighters.pulse();
  livePortraits.pulse();
});
const keyToBit={KeyZ:0,KeyX:8,KeyA:1,KeyS:9,ArrowUp:4,ArrowDown:5,ArrowLeft:6,ArrowRight:7,Enter:3,ShiftRight:2};
window.addEventListener('keydown',e=>{if(keyToBit[e.code]!==undefined){held|=1<<keyToBit[e.code];e.preventDefault();}});
window.addEventListener('keyup',e=>{if(keyToBit[e.code]!==undefined){held&=~(1<<keyToBit[e.code]);e.preventDefault();}});
for(const btn of document.querySelectorAll('button[data-bit]')) {
  const bit=+btn.dataset.bit, active=new Set();
  btn.addEventListener('pointerdown',e=>{e.preventDefault();active.add(e.pointerId);held|=1<<bit;btn.classList.add('pressed');btn.setPointerCapture(e.pointerId);});
  const release=e=>{active.delete(e.pointerId);if(!active.size){held&=~(1<<bit);btn.classList.remove('pressed');}};
  btn.addEventListener('pointerup',release);btn.addEventListener('pointercancel',release);btn.addEventListener('lostpointercapture',release);
}
$('sound').addEventListener('click', async () => {
  try {
    if (sound.enabled && sound.state === 'running') {
      await sound.mute();
      soundUi('已靜音');
    } else {
      await unlockAudio();
    }
  } catch (error) { soundUi(`播放失敗：${error.message}`); }
});
const stop=()=>{translationDiag=[];lastDiagSignature='';storyFrameGate.reset();hdFrameGate.reset();liveHd.detach();liveFighters.detach();livePortraits.detach();running=false;cancelAnimationFrame(raf);sound.reset();sound.mute().catch(console.warn);game=null;held=0;translationKey='';currentDialogue=null;embedded=false;$('translation-card').hidden=true;$('game').hidden=true;$('setup').hidden=false;$('start').disabled=false;};
$('stop').addEventListener('click',stop);
$('translation-debug').addEventListener('click',()=>{
  if (!game) return;
  sampleTranslationDiagnostics();
  const report = {
    format:'DBZ-SSD iPhone zh-TW diagnostic v1',
    branch:'iphone-web-poc',
    note:'只有畫面上擷取的英文字符及翻譯鍵值；不包含 ROM、IPS、HD ZIP 或存檔。',
    frame: game.frame_count(),
    events: translationDiag
  };
  const url=URL.createObjectURL(new Blob([JSON.stringify(report,null,2)],{type:'application/json'}));
  const a=document.createElement('a');
  a.href=url;a.download='dbz-translation-diagnostic.json';a.click();
  setTimeout(()=>URL.revokeObjectURL(url),3000);
});
document.addEventListener('visibilitychange',()=>{
  if(document.hidden&&running){running=false;cancelAnimationFrame(raf);held=0;sound.reset();}
  else if(!document.hidden&&game&&!running){
    running=true;lastTick=0;accumulator=0;raf=requestAnimationFrame(requestFrame);
    if(sound.state !== 'running') soundUi('Safari 已暫停聲音，請按「啟用聲音」');
  }
});
$('save').addEventListener('click',()=>{
  if(!game)return;
  const bytes=game.save_state();
  const url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'}));
  const a=document.createElement('a');a.href=url;a.download='dbz-ssd-web.sav';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
});
$('state-file').addEventListener('change',async e=>{
  if(!game||!e.target.files?.[0])return;
  try{ game.load_state(new Uint8Array(await e.target.files[0].arrayBuffer()));storyFrameGate.reset();hdFrameGate.reset();sound.reset();translationKey='';updateTranslation();window.alert('讀取存檔成功'); }
  catch(err){window.alert(String(err));}
  e.target.value='';
});
if('serviceWorker' in navigator && location.protocol.startsWith('http')) navigator.serviceWorker.register('./service-worker.js').catch(console.warn);
