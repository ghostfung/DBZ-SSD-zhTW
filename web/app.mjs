import {inspectRom} from './rom-check.mjs';
const $ = id => document.getElementById(id);
let moduleReady = null, game = null, held = 0, running = false, raf = 0;
let lastTick = 0, accumulator = 0, countWindow = 0, countFrames = 0;
const STEP = 1000/60.0988;
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
  try {
    $('start').disabled = true;
    const rom = new Uint8Array(await romInput.files[0].arrayBuffer());
    const ips = ipsInput.files?.[0] ? new Uint8Array(await ipsInput.files[0].arrayBuffer()) : new Uint8Array(0);
    const { WebGame } = await ensureEngine();
    game = new WebGame(rom, ips);
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
}
function requestFrame(now) {
  if (!running) return;
  try {
    if (!lastTick) lastTick = now;
    // Render and simulation clocks are independent, so 120 Hz displays don't speed up gameplay.
    accumulator += Math.min(now-lastTick, STEP*3);
    let n = 0;
    while(accumulator >= STEP && n < 3) { game.tick(held); countFrames++; n++; accumulator -= STEP; }
    drawFrame();
    $('frames').textContent=`${game.frame_count()} 幀`;
    if(!countWindow) countWindow=now;
    if(now-countWindow>=1000){$('fps').textContent=`約 ${(countFrames*1000/(now-countWindow)).toFixed(1)} 模擬幀/秒`;countWindow=now;countFrames=0;}
    lastTick=now;
    raf=requestAnimationFrame(requestFrame);
  } catch(e) { running=false; window.alert(`遊戲執行失敗：${e}`); }
}

const keyToBit={KeyZ:0,KeyX:8,KeyA:1,KeyS:9,ArrowUp:4,ArrowDown:5,ArrowLeft:6,ArrowRight:7,Enter:3,ShiftRight:2};
window.addEventListener('keydown',e=>{if(keyToBit[e.code]!==undefined){held|=1<<keyToBit[e.code];e.preventDefault();}});
window.addEventListener('keyup',e=>{if(keyToBit[e.code]!==undefined){held&=~(1<<keyToBit[e.code]);e.preventDefault();}});
for(const btn of document.querySelectorAll('button[data-bit]')) {
  const bit=+btn.dataset.bit, active=new Set();
  btn.addEventListener('pointerdown',e=>{e.preventDefault();active.add(e.pointerId);held|=1<<bit;btn.classList.add('pressed');btn.setPointerCapture(e.pointerId);});
  const release=e=>{active.delete(e.pointerId);if(!active.size){held&=~(1<<bit);btn.classList.remove('pressed');}};
  btn.addEventListener('pointerup',release);btn.addEventListener('pointercancel',release);btn.addEventListener('lostpointercapture',release);
}
const stop=()=>{running=false;cancelAnimationFrame(raf);game=null;held=0;$('game').hidden=true;$('setup').hidden=false;$('start').disabled=false;};
$('stop').addEventListener('click',stop);
document.addEventListener('visibilitychange',()=>{if(document.hidden&&running){running=false;cancelAnimationFrame(raf);held=0;}else if(!document.hidden&&game&&!running){running=true;lastTick=0;accumulator=0;raf=requestAnimationFrame(requestFrame);}});
$('save').addEventListener('click',()=>{
  if(!game)return;
  const bytes=game.save_state();
  const url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'}));
  const a=document.createElement('a');a.href=url;a.download='dbz-ssd-web.sav';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
});
$('state-file').addEventListener('change',async e=>{
  if(!game||!e.target.files?.[0])return;
  try{ game.load_state(new Uint8Array(await e.target.files[0].arrayBuffer()));window.alert('讀取存檔成功'); }
  catch(err){window.alert(String(err));}
  e.target.value='';
});
if('serviceWorker' in navigator && location.protocol.startsWith('http')) navigator.serviceWorker.register('./service-worker.js').catch(console.warn);
