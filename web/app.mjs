import {inspectRom} from './rom-check.mjs';
import {SnesAudio} from './audio.mjs';
import {findDialogueBox,drawChineseInBox,maskPendingDialogue} from './chinese-overlay.mjs';
import {assembleTextRuns,reconcileDialogue,drawMenuTranslations} from './screen-text.mjs';
import {LiveHdArt} from './hd-live.mjs';
const $ = id => document.getElementById(id);
let moduleReady = null, game = null, held = 0, running = false, raf = 0;
let lastTick = 0, accumulator = 0, countWindow = 0, countFrames = 0;
let lastHdPoll = 0, translationKey = '';
let currentDialogue = null, embedded = false, hdEnabled = false;
let observedRuns = [], menuNames = {};
const STEP = 1000/60.0988;
const sound = new SnesAudio();
const liveHd = new LiveHdArt(message => { $('hd-live-info').textContent = message; });
document.addEventListener('hd-catalog-ready', event => {
  liveHd.setCatalog(event.detail.catalog, event.detail.assets);
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
    const translatedRows = game.load_translation(await translationResponse.text());
    menuNames = JSON.parse(game.menu_translations());
    // Use the author's existing Traditional Chinese names for original battle
    // status tiles. These are drawn outside the message engine and may run
    // directly into numerical HP values (e.g. "Piccolo77").
    for (const [english,key] of [['Goku','string:0'],['Piccolo','string:2']]) {
      const translated = game.translation_for_key(key);
      if (translated && !menuNames[english]) menuNames[english] = translated;
    }
    observedRuns = [];
    $('translation-count').textContent = '已載入 ' + translatedRows + ' 筆繁體中文譯文';
    translationKey = '';
    currentDialogue = null;
    embedded = false;
    game.set_hd_mode(hdEnabled);
    liveHd.attach(game);
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
  if (w===0||h===0) return;
  if (canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;}
  const rgba=game.frame_rgba();
  if (rgba.length!==w*h*4) throw Error('核心輸出的 RGBA 畫面大小錯誤');
  ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba),w,h),0,0);
  const insideGame = Boolean($('inframe')?.checked);
  // Even when the SNES message engine is between pages, the PPU can already
  // display the next English glyphs. Inspect the frame independently of the
  // translation key, then mask unresolved transition text before presenting it.
  const dialogBox = insideGame ? findDialogueBox(rgba,w,h) : null;
  const painted = Boolean(dialogBox && insideGame && currentDialogue &&
    drawChineseInBox(ctx,rgba,w,h,currentDialogue,dialogBox,observedRuns));
  if (insideGame && dialogBox && !painted) {
    maskPendingDialogue(ctx,w,h,dialogBox,observedRuns,menuNames);
  }
  // Native SNES menu labels do not pass through the message engine. The
  // PPU text capture lets us replace only words actually visible on screen.
  drawMenuTranslations(ctx,w,h,observedRuns,menuNames,dialogBox);
  if(embedded!==Boolean(painted)){
    embedded=Boolean(painted);
    $('translation-card').hidden=embedded || !currentDialogue;
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
    drawFrame();
    if (now - lastHdPoll >= 450) {
      lastHdPoll = now;
      liveHd.pulse();
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
  if(!game){
    currentDialogue=null;
    observedRuns=[];
    box.hidden=true;
    return;
  }
  try {
    observedRuns=assembleTextRuns(JSON.parse(game.visible_text_cells()));
  } catch(error) {
    observedRuns=[];
    console.warn('Unable to inspect current PPU text',error);
  }
  const serialized=game.current_dialogue();
  const detected=serialized ? JSON.parse(serialized) : null;
  // A stopped message key can survive in WRAM after the next English text
  // has appeared. Resolve that mismatch using the actually rendered glyphs.
  const msg=reconcileDialogue(detected,observedRuns,key=>game.translation_for_key(key));
  if(!msg?.text){
    currentDialogue=null;
    embedded=false;
    box.hidden=true;
    translationKey='';
    return;
  }
  currentDialogue=msg;
  const signature=msg.key+'|'+msg.text;
  if(translationKey!==signature){
    $('translation-text').textContent=msg.text;
    $('translation-key').textContent=msg.key;
    translationKey=signature;
  }
  box.hidden=embedded;
}
$('inframe').addEventListener('change', () => {
  embedded = false;
  if (game) updateTranslation();
});
$('hd-toggle').addEventListener('click', () => {
  hdEnabled = !hdEnabled;
  $('hd-toggle').textContent = hdEnabled ? '關閉 HD 2x' : '啟用 HD 2x';
  $('hd-info').textContent = hdEnabled ? '正在初始化 HD 渲染…' : 'Classic 原版畫面';
  if (game) game.set_hd_mode(hdEnabled);
  liveHd.pulse();
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
const stop=()=>{liveHd.detach();observedRuns=[];menuNames={};running=false;cancelAnimationFrame(raf);sound.reset();sound.mute().catch(console.warn);game=null;held=0;translationKey='';currentDialogue=null;embedded=false;$('translation-card').hidden=true;$('game').hidden=true;$('setup').hidden=false;$('start').disabled=false;};
$('stop').addEventListener('click',stop);
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
  try{ game.load_state(new Uint8Array(await e.target.files[0].arrayBuffer()));sound.reset();translationKey='';updateTranslation();window.alert('讀取存檔成功'); }
  catch(err){window.alert(String(err));}
  e.target.value='';
});
if('serviceWorker' in navigator && location.protocol.startsWith('http')) navigator.serviceWorker.register('./service-worker.js').catch(console.warn);
