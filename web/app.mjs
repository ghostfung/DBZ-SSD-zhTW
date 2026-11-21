import {inspectRom} from './rom-check.mjs';
import {SnesAudio} from './audio.mjs';
import {saveGameFile,readGameFile,deleteGameFile} from './local-game-files.mjs';
import {LiveHdArt} from './hd-live.mjs';
import {StoryFrameGate} from './story-frame-gate.mjs';
import {HdFrameGate} from './hd-frame-gate.mjs';
import {PortraitFrameGate} from './hd-portrait-gate.mjs';
import {LiveHdFighters} from './hd-fighter-live.mjs';
import {LiveHdPortraits} from './hd-portrait-live.mjs';
import {LiveHdSpecials} from './hd-special-live.mjs';
import {LiveHdMaps} from './hd-map-live.mjs';
import {LiveHdCards} from './hd-card-live.mjs';
import {LiveHdMode7} from './hd-mode7-live.mjs';
import {FrameMetrics} from './frame-metrics.mjs';
import {HdPresentationBudget} from './hd-presentation-budget.mjs';
import {attachHdRuntimeSnapshot,HdSceneTrace} from './hd-coverage-runtime.mjs?build=v47';
import {describeHdPhases} from './hd-phase-report.mjs';
import {TransitionArtifactGate} from './transition-artifact-gate.mjs';
const BUILD_ID='dbz-hd-tw-pwa-v49';
const $ = id => document.getElementById(id);
let moduleReady = null, game = null, held = 0, running = false, raf = 0;
let lastTick = 0, accumulator = 0, countWindow = 0, countFrames = 0;
let averageTickMs = 0;
let lastHdPoll = 0, lastDiagPoll = 0, translationKey = '';
let currentDialogue = null, embedded = false, hdEnabled = false, classicZhEnabled = false;
let hdPreference=null;
try {
  const pref=localStorage.getItem('dbz-hd-mode');
  if(pref==='hd'||pref==='classic'||pref==='classic-zh'){
    hdPreference=pref;hdEnabled=pref==='hd';classicZhEnabled=pref==='classic-zh';
  }
}catch {}
let savedRom=null,savedIps=null;
let language='zh-tw';
try { language=localStorage.getItem('dbz-hd-language')==='zh-hk'?'zh-hk':'zh-tw'; }catch {}
const translationCache=new Map();
function updateRenderModeButton(){
  const button=$('hd-toggle');
  button.textContent=hdEnabled?'HD 2x':classicZhEnabled?'中文 1x':'原版';
  button.setAttribute('aria-label',
    '畫面模式：'+(hdEnabled?'HD 2x':classicZhEnabled?'Classic 繁體中文 1x':'Classic 英文原版')+'，點擊切換');
}
function updateLanguageButton(){
  $('locale-toggle').textContent=language==='zh-hk'?'香港繁中':'台灣繁中';
  $('locale-toggle').setAttribute('aria-label','文字版本：'+$('locale-toggle').textContent+'，點按切換');
}
async function getTranslation(code){
  if(!['zh-tw','zh-hk'].includes(code))throw Error('不支援的翻譯版本');
  if(translationCache.has(code))return translationCache.get(code);
  const [translationResponse,alignResponse]=await Promise.all([
    fetch('./'+code+'.tsv'),fetch('./zh-tw-page-alignments.tsv')
  ]);
  if(!translationResponse.ok)throw Error('翻譯檔不存在（HTTP '+translationResponse.status+'）');
  if(!alignResponse.ok)throw Error('繁體中文分頁對齊檔載入失敗');
  const contents=(await translationResponse.text()).trimEnd()+'\n'+
    (await alignResponse.text()).trim();
  translationCache.set(code,contents);
  return contents;
}

const STEP = 1000/60.0988;
const frameMetrics = new FrameMetrics();
const transientHoldBudget = new HdPresentationBudget();
let perfHistory=[];
const hdSceneTrace = new HdSceneTrace();
let lastPresentedFrame=null,lastPresentation=null;
const transitionArtifactGate = new TransitionArtifactGate({maxFrames:5});
const sound = new SnesAudio();
const liveHd = new LiveHdArt(message => { $('hd-live-info').textContent = message; });
const liveFighters = new LiveHdFighters(message => { $('hd-fighter-info').textContent = message; });
const livePortraits = new LiveHdPortraits(message => { $('hd-portrait-info').textContent = message; });
const liveSpecials = new LiveHdSpecials(message => { $('hd-special-info').textContent = message; });
const liveMaps = new LiveHdMaps(message => { $('hd-map-info').textContent = message; });
const liveCards = new LiveHdCards(message => { $('hd-card-info').textContent = message; });
const liveMode7 = new LiveHdMode7(message => { $('hd-mode7-info').textContent = message; });
const storyFrameGate = new StoryFrameGate(10);
const hdFrameGate = new HdFrameGate();
const portraitFrameGate = new PortraitFrameGate();
const fighterFrameGate = new PortraitFrameGate({pendingFrames:36,transitionFrames:5});
const specialFrameGate = new PortraitFrameGate({pendingFrames:24,transitionFrames:4});
const mapFrameGate = new PortraitFrameGate({pendingFrames:48,transitionFrames:6});
const cardFrameGate = new PortraitFrameGate({pendingFrames:30,transitionFrames:5});
const mode7FrameGate = new PortraitFrameGate({pendingFrames:48,transitionFrames:6});

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
    item.hd_special_pixels = game.hd_special_pixels?.() ?? 0;
    item.hd_map_pixels = game.hd_map_pixels?.() ?? 0;
    item.hd_card_pixels = game.hd_card_pixels?.() ?? 0;
    item.hd_mode7_pixels = game.hd_mode7_pixels?.() ?? 0;
    item.map_holding = mapFrameGate.isHolding;
    item.special_holding = specialFrameGate.isHolding;
    item.portrait_holding = portraitFrameGate.isHolding;
    item.portrait_held_frames = portraitFrameGate.heldFramesTotal;
    try { item.portrait_probe = JSON.parse(game.portrait_probe?.() || '{}'); } catch {}
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


document.addEventListener('hd-coverage-runtime-snapshot',event=>{
  attachHdRuntimeSnapshot(event.detail,game);
  if(event.detail && typeof event.detail==='object'){
    const core=game?.browser_build_tag?.()??null;
    event.detail.build={client:BUILD_ID,core,matched:core===BUILD_ID};
    event.detail.recentSceneTrace=hdSceneTrace.report();
    // Two-second browser timing windows, bounded to the most recent minute.
    // No ROM, HD files, save data, device IDs or pixel arrays are exported.
    event.detail.framePerformance={windowMs:2000,
      windows:perfHistory.slice(),transientHoldBudget:transientHoldBudget.summary()};
  }
});

document.addEventListener('hd-catalog-ready', event => {
  liveHd.setCatalog(event.detail.catalog, event.detail.assets);
  // First import defaults to HD; respect any previous explicit Classic pick.
  if(event.detail.assets&&!game&&hdPreference===null){
    hdEnabled=true;classicZhEnabled=false;
    updateRenderModeButton();
  }
  hdFrameGate.reset();
});
document.addEventListener('hd-fighter-catalog-ready', event => {
  liveFighters.setCatalog(event.detail.catalog, event.detail.assets);
  fighterFrameGate.reset();
});
document.addEventListener('hd-mode7-catalog-ready',event=>{
  liveMode7.setCatalog(event.detail.catalog,event.detail.assets);
  mode7FrameGate.reset();
});
document.addEventListener('hd-card-catalog-ready', event => {
  liveCards.setCatalog(event.detail.catalog,event.detail.assets);
  cardFrameGate.reset();
});
document.addEventListener('hd-map-catalog-ready', event => {
  liveMaps.setCatalog(event.detail.catalog,event.detail.assets);
  mapFrameGate.reset();
});
document.addEventListener('hd-special-catalog-ready', event => {
  liveSpecials.setCatalog(event.detail.catalog,event.detail.assets);
  specialFrameGate.reset();
});
document.addEventListener('hd-portrait-catalog-ready', event => {
  livePortraits.setCatalog(event.detail.catalog, event.detail.assets);
  portraitFrameGate.reset();
});

// A WebKit context can be marked 'running' while completely inaudible after
// an app switch. Keep manual recovery accessible until the player chooses it.
let offerManualSoundRecovery=false;
function soundUi(message) {
  const ready=sound.enabled && sound.state==='running';
  $('sound').hidden=!game||(ready&&!offerManualSoundRecovery);
  $('sound').textContent=offerManualSoundRecovery?'重新連接聲音':'恢復聲音';
  $('sound-status').textContent=message||
    (offerManualSoundRecovery
      ? '切換 App 後若無聲，請按「重新連接聲音」'
      : ready?`聲音已啟用 · ${sound.sampleRate} Hz`
        : '聲音已暫停，點「恢復聲音」');
}
async function restoreAudioAfterInterruption() {
  if(!game)return false;
  if(!sound.enabled){
    soundUi('聲音未啟用；點「恢復聲音」允許 Safari 播放');
    return false;
  }
  try {
    const ready=await sound.resumeAfterInterruption();
    if(ready)game.set_audio_rate(sound.sampleRate);
    soundUi(ready && offerManualSoundRecovery
      ? '已嘗試恢復；如仍無聲，點「重新連接聲音」'
      : ready?'聲音已恢復':undefined);
    return ready;
  }catch(error){
    soundUi('iPhone 尚未允許聲音恢復：'+(error?.message||error));
    return false;
  }
}
let toastTimeout;
function gameToast(message){
  const toast=$('play-toast');
  clearTimeout(toastTimeout);
  toast.textContent=message;toast.hidden=false;
  toastTimeout=setTimeout(()=>{toast.hidden=true;},2800);
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
// HD is validated from the player's local OPFS bytes on every page load.
// The byte progress event is separate from coarse, non-time-based launch stages.
let latestHdRestoreProgress=null;
let activeHdRestoreReporter=null;
document.addEventListener('hd-restore-progress',event=>{
  const done=event.detail?.done,total=event.detail?.total;
  if(!Number.isFinite(done)||!Number.isFinite(total)||total<=0)return;
  latestHdRestoreProgress={
    done,total,percent:Math.max(0,Math.min(100,Math.floor(done/total*100)))
  };
  activeHdRestoreReporter?.(latestHdRestoreProgress);
});
function refreshLaunchCover({edit=false}={}){
  // Returning players see one title image and Start; a small management
  // control still permits replacing locally stored ROM/IPS/HD files.
  const ready=Boolean(savedRom&&!$('rom').files?.length&&!edit);
  $('setup').classList.toggle('launch-ready',ready);
  $('setup-edit').hidden=!ready;
}
$('setup-edit').addEventListener('click',()=>{
  $('setup').classList.remove('launch-ready');
  $('setup-edit').hidden=true;
  $('game-files-details').open=true;
  $('setup-files').scrollIntoView?.({block:'start',behavior:'smooth'});
});
const romInput = $('rom'), ipsInput = $('ips');

function displayStoredFiles(){
  $('local-files-status').textContent=savedRom
    ? '已保存 ROM：'+savedRom.name+(savedIps?'；IPS：'+savedIps.name:'；無 IPS')+
      '。下次從同一個 PWA 網址開啟，不用再選遊戲檔案。'
    : '首次使用請選 ROM，日文 Rev 1 ROM 另需選 IPS。';
}
romInput.addEventListener('change', async () => {
  const file=romInput.files?.[0];
  if(!file)return;
  try {
    const check=inspectRom(await file.arrayBuffer());
    setStatus(`${check.label}\n${check.crc ? `CRC32: ${check.crc}` : ''}`);
    const valid=check.kind!=='invalid'&&check.kind!=='unknown';
    $('start').disabled=!valid;
    if(!valid)return;
    // Changing the ROM must not automatically reuse an old game's IPS.
    savedRom=null;savedIps=null;ipsInput.value='';
    await deleteGameFile('ips').catch(()=>{});
    try {
      await saveGameFile('rom',file);
      savedRom=await readGameFile('rom');
      displayStoredFiles();
      refreshLaunchCover({edit:true});
    }catch(error){
      setStatus(check.label+'。ROM 可於本次開啟使用，但保存失敗：'+error.message);
    }
  }catch(error){$('start').disabled=true;setStatus('ROM 檢查失敗：'+error.message);}
});
ipsInput.addEventListener('change',async()=>{
  const file=ipsInput.files?.[0];
  if(!file)return;
  try{
    await saveGameFile('ips',file);
    savedIps=await readGameFile('ips');
    displayStoredFiles();
  }catch(error){
    setStatus('IPS 可於本次開啟使用，但保存失敗：'+error.message);
  }
});
$('clear-game-files').addEventListener('click',async()=>{
  await Promise.all([deleteGameFile('rom'),deleteGameFile('ips')]).catch(error=>
    setStatus('本機遊戲檔案清理失敗：'+error.message));
  savedRom=null;savedIps=null;romInput.value='';ipsInput.value='';
  $('start').disabled=true;$('game-files-details').open=true;
  displayStoredFiles();
  refreshLaunchCover({edit:true});
});
void (async()=>{
  try {
    const [rom,ips]=await Promise.all([readGameFile('rom'),readGameFile('ips')]);
    // Never override a new selection while an IndexedDB load was in flight.
    if(rom&&!romInput.files?.length){
      const check=inspectRom(await rom.blob.arrayBuffer());
      if(check.kind!=='invalid'&&check.kind!=='unknown'){
        savedRom=rom;savedIps=ips;
        $('start').disabled=false;
        $('game-files-details').open=false;
        setStatus('已從 PWA 本機載入遊戲 ROM，可直接開始遊戲。');
        refreshLaunchCover();
      }
    }
  }catch(error){setStatus('本機遊戲檔案無法恢復：'+error.message+'；請重新選擇 ROM。');}
  displayStoredFiles();
})();

// Make the 18 MiB WASM load visible. It must never be blocked by the
// service worker's local cache write. Explicit versioned URLs prevent stale
// iOS Safari HTTP-cache responses from mixing two different browser cores.
async function ensureEngine(phase=async()=>{}) {
  if (moduleReady) return moduleReady;
  try {
    await phase('正在載入遊戲程式模組…');
    const mod = await import('./pkg/ssd_web.js?build=v49');
    // Use the wasm-pack loader from the previously working v41 release.
    // Its versioned URL can stream normally, with no manually-created
    // Response passed across WebKit's Service Worker boundary.
    await phase('正在直接下載及編譯 WASM 遊戲核心（約 18 MB）…');
    const wasmUrl=new URL('./pkg/ssd_web_bg.wasm?build=v49',import.meta.url);
    await mod.default({module_or_path:wasmUrl});
    moduleReady = mod;
    return mod;
  } catch (error) {
    throw Error('WASM 初始化失敗：'+(error?.name==='AbortError'?
      '網路下載超時。請檢查網路後重試。':error?.message||String(error)));
  }
}

$('start').addEventListener('click', async () => {
  // Safari audio unlock must begin synchronously inside the user's tap.
  unlockAudio();
  const button=$('start'),originalLabel='啟動遊戲';
  const compactLaunch=$('setup').classList.contains('launch-ready');
  $('setup').classList.add('launching');
  $('setup').classList.remove('launch-failed');
  let currentStage='讀取遊戲檔案';
  // A stage percentage counts completed setup stages, not elapsed time.
  // HD hashing below has its own genuinely byte-measured percentage.
  const stageCount=7;
  let stageIndex=0;
  const phase=async message=>{
    currentStage=message;
    stageIndex=Math.min(stageCount,stageIndex+1);
    const percent=Math.floor((stageIndex-1)/stageCount*100);
    button.textContent=`啟動中 · ${percent}%`;
    setStatus(`啟動步驟 ${stageIndex}/${stageCount}（已完成 ${percent}% 步驟）：${message}`);
    // Paint the updated percent before Safari performs expensive WASM work.
    await new Promise(resolve=>setTimeout(resolve,0));
  };
  try {
    button.disabled = true;
    if(!romInput.files?.[0]&&!savedRom)throw Error('未選取遊戲 ROM，請先選擇 ROM 檔案');
    await phase('正在讀取 ROM／IPS…');
    const rom = new Uint8Array(await (romInput.files?.[0]??savedRom.blob).arrayBuffer());
    const ipsFile=ipsInput.files?.[0]??savedIps?.blob;
    const ips = ipsFile ? new Uint8Array(await ipsFile.arrayBuffer()) : new Uint8Array(0);
    // Cached HD ZIP may still be hashing when the player presses Start.
    // Wait for completed verification before attaching any HD catalogs.
    if(globalThis.hdRestorePromise){
      await phase('正在準備本機 HD 美術包…');
      activeHdRestoreReporter=({done,total,percent})=>{
        button.textContent=`HD 驗證 · ${percent}%`;
        setStatus(`正在驗證已保存的 HD 美術包：${percent}%（${(done/1048576).toFixed(0)} / ${(total/1048576).toFixed(0)} MiB）`);
      };
      try{
        // A hash may already be underway when Start is tapped.
        if(latestHdRestoreProgress)activeHdRestoreReporter(latestHdRestoreProgress);
        await globalThis.hdRestorePromise;
      }finally{
        activeHdRestoreReporter=null;
      }
    }
    const { WebGame } = await ensureEngine(phase);
    await phase('正在建立遊戲核心…');
    game = new WebGame(rom, ips);
    const coreBuild=game.browser_build_tag?.()??null;
    if(coreBuild!==BUILD_ID){
      game.free?.();game=null;
      throw Error('網頁與 WASM 核心版本不一致：頁面 '+BUILD_ID+'；核心 '+
        (coreBuild??'舊版未標記')+'。請重新開啟最新測試網址。');
    }
    $('build-status').textContent='頁面／WASM 一致：'+BUILD_ID;
    // Official TW/HK translations, both sourced from Ming Liu's tables.
    await phase('正在載入'+(language==='zh-hk'?'香港':'台灣')+'繁體中文譯文…');
    const translatedRows=game.load_translation(await getTranslation(language));
    $('translation-count').textContent = '已載入 ' + translatedRows + ' 筆繁體中文譯文';
    translationKey = '';
    currentDialogue = null;
    embedded = false;
    translationDiag = []; lastDiagSignature = '';
    storyFrameGate.reset();
    hdFrameGate.reset();
    portraitFrameGate.reset();
    fighterFrameGate.reset();
    specialFrameGate.reset();
    mapFrameGate.reset();
    cardFrameGate.reset();
    mode7FrameGate.reset();
    transitionArtifactGate.reset();
    game.set_hd_mode(hdEnabled);
    game.set_classic_zh_mode(classicZhEnabled);
    game.set_fighter_hd_enabled($('fighter-hd-toggle').checked);
    game.set_special_hd_enabled?.($('special-hd-toggle').checked);
    game.set_map_hd_enabled?.($('map-hd-toggle').checked);
    game.set_card_hd_enabled?.($('card-hd-toggle').checked);
    game.set_mode7_hd_enabled?.($('mode7-hd-toggle').checked);
    game.set_portrait_hd_enabled?.($('portrait-hd-toggle').checked);
    await phase('正在套用 HD 美術索引…');
    liveHd.attach(game);
    liveFighters.attach(game);
    liveSpecials.attach(game);
    liveMaps.attach(game);
    liveCards.attach(game);
    liveMode7.attach(game);
    livePortraits.attach(game);
    if (sound.enabled && sound.state === 'running') game.set_audio_rate(sound.sampleRate);
    soundUi();
    $('setup').classList.remove('launching');
    $('setup').hidden = true;
    $('game').hidden = false;
    document.body.classList.add('playing');
    updateRenderModeButton();
    button.textContent=originalLabel;
    held = 0; running = true; lastTick = 0; accumulator = 0; countWindow = 0; countFrames = 0;
    frameMetrics.reset();perfHistory=[];transientHoldBudget.reset();
    hdSceneTrace.reset();lastPresentedFrame=null;lastPresentation=null;
    averageTickMs=0;
    transitionArtifactGate.reset();
    $('frame-perf').textContent='卡頓監測：正在收集影格耗時…';
    $('hd-phase-perf').textContent='HD 分段耗時：正在採樣…';
    raf = requestAnimationFrame(requestFrame);
  } catch (error) {
    button.disabled=false;
    button.textContent=originalLabel;
    $('setup').classList.remove('launching');
    $('setup').classList.add('launch-failed');
    setStatus('遊戲啟動失敗（'+currentStage+'）：'+(error?.message||String(error)));
    // A failed translation, index or version check must not leave a
    // partially initialized WebGame alive across the next start attempt.
    if(!running && game){
      try {game.free?.();} catch {}
      game=null;
    }
  }
});

const ctx = $('screen').getContext('2d', {alpha:false});
// Reuse one ~1 MiB pixel buffer and ImageData across frames. Mobile Safari
// previously allocated two new full-frame buffers on every 60 Hz repaint.
let frameBytes=null, frameImage=null;
function drawFrame() {
  const w=game.width(), h=game.height();
  const canvas=$('screen');
  if (w===0 || h===0) return;
  if (canvas.width!==w || canvas.height!==h) {
    canvas.width=w; canvas.height=h;
    canvas.parentElement.style.setProperty('--game-ratio',w+' / '+h);
    lastPresentedFrame=null;
    storyFrameGate.reset();
    hdFrameGate.reset();
    portraitFrameGate.reset();
    specialFrameGate.reset();
    mapFrameGate.reset();
    cardFrameGate.reset();
    mode7FrameGate.reset();
    transitionArtifactGate.reset();
  }
  // A fast advance can expose the next page's native English glyphs before
  // the Rust stateful Chinese renderer has filled that frame. Hold only the
  // *last presented picture* for up to ten simulated frames, not the game:
  // inputs, sound and emulated time continue without interruption.
  const pixels=game.hd_art_pixels();
  const enhanced=game.hd_active();
  // Bitmask computed in Rust without JSON serialization. Most visible HD
  // assets are already decoded; only query expensive matches when pending.
  const pendingMask=game.hd_pending_mask?.();
  const mayNeed=bit=>pendingMask===undefined || (pendingMask & bit)!==0;
  const storyHold=storyFrameGate.shouldDefer(
    game.frame_count(), game.story_translation_pending(), enhanced
  );
  // A new PPU background can be matched a few frames before its local 2x
  // picture finishes decoding. Keep the last HD picture temporarily instead
  // of painting the raw SNES background. Never hold indefinitely.
  let pendingArt=false;
  if (enhanced && hdEnabled && liveHd.assets && pixels===0 && mayNeed(64)) {
    try {
      pendingArt=JSON.parse(game.background_matches()).some(m=>!m.ready);
    } catch { /* Corrupt/absent catalogue: fail open to the original frame. */ }
  }
  const hdHold=hdFrameGate.shouldDefer(game.frame_count(),{
    enabled:enhanced && hdEnabled && Boolean(liveHd.assets),
    pixels,pending:pendingArt
  });
  // Only a verified on-screen class1 candidate may delay a raw portrait.
  // This is an independent, bounded presentation hold: music, input and
  // emulator clocks continue, including if the ZIP or art is missing.
  let portraitMatches=[];
  if(enhanced && hdEnabled && livePortraits.assets && mayNeed(2) &&
      $('portrait-hd-toggle').checked && typeof game.portrait_matches==='function'){
    try { portraitMatches=JSON.parse(game.portrait_matches()); }
    catch { portraitMatches=[]; }
  }
  let fighterMatches=[];
  if(enhanced && hdEnabled && liveFighters.assets && mayNeed(1) && $('fighter-hd-toggle').checked &&
    typeof game.fighter_matches==='function'){
    try{fighterMatches=JSON.parse(game.fighter_matches());}catch{}
  }
  const fighterHold=fighterFrameGate.shouldDefer(game.frame_count(),{
    // No matched pose means no HD silhouette to preserve; do not freeze
    // an unrelated preceding frame during fast-moving field animations.
    enabled:enhanced && hdEnabled && Boolean(liveFighters.assets) &&
      $('fighter-hd-toggle').checked && fighterMatches.length>0,
    pending:fighterMatches.some(m=>!m.ready),
    ready:fighterMatches.some(m=>m.ready),
    pixels:game.hd_fighter_pixels?.()??0,
    key:fighterMatches.map(m=>m.index+':'+m.key).sort().join('|')
  });
  const portraitHold=portraitFrameGate.shouldDefer(game.frame_count(),{
    enabled:enhanced && hdEnabled && Boolean(livePortraits.assets) &&
      $('portrait-hd-toggle').checked,
    pending:portraitMatches.some(m=>!m.ready),
    ready:portraitMatches.some(m=>m.ready),
    pixels:game.hd_portrait_pixels?.()??0,
    key:portraitMatches.map(m=>m.index+':'+m.x+':'+m.y).sort().join('|')
  });
  let specialMatches=[];
  if(enhanced && hdEnabled && liveSpecials.assets && mayNeed(4) &&
      $('special-hd-toggle').checked && typeof game.special_matches==='function') {
    try {specialMatches=JSON.parse(game.special_matches());}catch{}
  }
  const specialHold=specialFrameGate.shouldDefer(game.frame_count(),{
    // Many transient OAM effects disappear naturally between poses.
    // A disappeared effect must not freeze the entire picture on handoff.
    enabled:enhanced && hdEnabled && Boolean(liveSpecials.assets) &&
      $('special-hd-toggle').checked && specialMatches.length>0,
    pending:specialMatches.some(m=>!m.ready),
    ready:specialMatches.some(m=>m.ready),
    pixels:game.hd_special_pixels?.()??0,
    key:specialMatches.map(m=>m.index+':'+m.key).sort().join('|')
  });
  let mapMatches=[];
  if(enhanced && hdEnabled && liveMaps.assets && mayNeed(8) && $('map-hd-toggle').checked &&
    typeof game.map_matches==='function'){
    try{mapMatches=JSON.parse(game.map_matches());}catch{}
  }
  const mapHold=mapFrameGate.shouldDefer(game.frame_count(),{
    enabled:enhanced && hdEnabled && Boolean(liveMaps.assets) &&
      $('map-hd-toggle').checked,
    pending:mapMatches.some(m=>!m.ready),
    ready:mapMatches.some(m=>m.ready),
    pixels:game.hd_map_pixels?.()??0,
    key:mapMatches.map(m=>m.index+':'+m.tileX+':'+m.tileY).sort().join('|')
  });
  let cardMatches=[];
  if(enhanced && hdEnabled && liveCards.assets && mayNeed(16) && $('card-hd-toggle').checked &&
    typeof game.card_matches==='function'){
    try{cardMatches=JSON.parse(game.card_matches());}catch{}
  }
  const cardHold=cardFrameGate.shouldDefer(game.frame_count(),{
    enabled:enhanced && hdEnabled && Boolean(liveCards.assets) &&
      $('card-hd-toggle').checked,
    pending:cardMatches.some(m=>m.layers?.some(l=>!l.ready)),
    ready:cardMatches.some(m=>m.layers?.some(l=>l.ready)),
    pixels:game.hd_card_pixels?.()??0,
    key:cardMatches.map(m=>m.index+':'+m.x+':'+m.y).sort().join('|')
  });
  let mode7Matches=[];
  if(enhanced && hdEnabled && liveMode7.assets && mayNeed(32) && $('mode7-hd-toggle').checked &&
    typeof game.mode7_matches==='function'){
    try{mode7Matches=JSON.parse(game.mode7_matches());}catch{}
  }
  const mode7Hold=mode7FrameGate.shouldDefer(game.frame_count(),{
    enabled:enhanced && hdEnabled && Boolean(liveMode7.assets) &&
      $('mode7-hd-toggle').checked && mode7Matches.length>0,
    pending:mode7Matches.some(m=>!m.ready),
    ready:mode7Matches.some(m=>m.ready),
    pixels:game.hd_mode7_pixels?.()??0,
    key:mode7Matches.map(m=>m.index+':'+m.x+':'+m.y).sort().join('|')
  });
  const holdReasons=[['story',storyHold],['background',hdHold],
    ['fighter',fighterHold],['portrait',portraitHold],['special',specialHold],
    ['map',mapHold],['cards',cardHold],['mode7',mode7Hold]]
    .filter(([,held])=>held).map(([name])=>name);
  // Keep important background/translation hand-offs unchanged. For
  // transient sprites, cap both consecutive frozen frames and the rolling
  // amount of frozen video, even if different HD poses keep arriving.
  const transientOnly=holdReasons.length>0&&!storyHold&&!hdHold;
  const allowTransient=transientHoldBudget.allow(game.frame_count(),transientOnly);
  const bypassedTransient=transientOnly&&!allowTransient;
  if(bypassedTransient)holdReasons.length=0;
  if (holdReasons.length===0) {
    const rgba=game.frame_rgba();
    if (rgba.length!==w*h*4) throw Error('核心輸出的 RGBA 畫面大小錯誤');
    // A loaded HD background can still be composited over an incomplete
    // transition PPU frame: sparse pixels / lone sprites over black.
    // Bound the hold to five simulation frames. Do not pause emulation, and
    // let real all-black fades pass through without interruption.
    const artifactHold=transitionArtifactGate.shouldDefer(
      game.frame_count(),rgba,w,h,enhanced && hdEnabled);
    if(artifactHold)holdReasons.push('transition-artifact');
    if(!artifactHold){
      if (!frameBytes || frameBytes.length!==rgba.length) {
        frameBytes=new Uint8ClampedArray(rgba.length);
        frameImage=new ImageData(frameBytes,w,h);
      }
      frameBytes.set(rgba);
      ctx.putImageData(frameImage,0,0);
      lastPresentedFrame=game.frame_count();
      transitionArtifactGate.didPresent(rgba,w,h);
      storyFrameGate.didPresent();
      hdFrameGate.didPresent(pixels);
      portraitFrameGate.didPresent();
      fighterFrameGate.didPresent();
      specialFrameGate.didPresent();
      mapFrameGate.didPresent();
      cardFrameGate.didPresent();
      mode7FrameGate.didPresent();
    }
  }
  lastPresentation={displayedFrame:lastPresentedFrame,
    held:holdReasons.length>0,reasons:holdReasons,
    bypassedTransient};
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
  // The 1x original pixels are duplicated exactly in Rust; the 2x Chinese
  // glyph antialiasing already lives in that buffer. Keep pixel-art sampling.
  canvas.style.imageRendering=game.hd_active()?'auto':'pixelated';
}
function requestFrame(now) {
  if (!running) return;
  try {
    if (!lastTick) lastTick = now;
    // Render and simulation clocks are independent, so 120 Hz displays don't speed up gameplay.
    // Avoid a spiral of death on heavy Mode 7 and scene transitions:
    // three full 2x PPU renders in one RAF caused repeated multi-frame stalls.
    // Keep at most two simulation steps and discard stale backlog rather
    // than chasing elapsed wall-clock time with an unbounded burst.
    const rafGap=now-lastTick;
    accumulator += Math.min(rafGap, STEP*2);
    let n = 0;
    // A slow HD render cannot catch up by running two expensive frames in
    // the same RAF. Adaptive cap avoids 43 ms simulation bursts while fast
    // frames still recover up to two missed steps.
    const maxCatchup=averageTickMs>=STEP*.90 ? 1 : 2;
    const simulationStart=performance.now();
    while(accumulator >= STEP && n < maxCatchup) {
      game.tick(held);countFrames++;n++;accumulator -= STEP;
    }
    const simulationMs=performance.now()-simulationStart;
    if(n){
      const perFrame=simulationMs/n;
      averageTickMs=averageTickMs===0 ? perFrame :
        averageTickMs*.75+perFrame*.25;
    }
    if (n) {
      sound.enqueue(game.drain_audio());
      // Synchronise the message with the PPU from the same emulation frame.
      // The former 80 ms timer displayed bare English before Chinese.
      updateTranslation();
      // A face may enter on a single battle/cutscene frame. Queue locally
      // matched art on that same emulated frame, not 90 ms later.
      // Skip six synchronous JS/WASM JSON roundtrips once matched images
      // are ready. Pending bits are set within the same emulated frame.
      if(hdEnabled){
        const mask=game.hd_pending_mask?.()??63;
        if(mask&64)liveHd.pulse(true);
        if(mask&1)liveFighters.pulse(true);
        if(mask&2)livePortraits.pulse(true);
        if(mask&4)liveSpecials.pulse(true);
        if(mask&8)liveMaps.pulse(true);
        if(mask&16)liveCards.pulse(true);
        if(mask&32)liveMode7.pulse(true);
      }
    }
    // Accelerate background catalogue polling only while a new HD layer is
    // missing; 90 ms avoids waiting for the old 450 ms idle interval.
    // Background image requests now happen on the same frame via bit 64.
    // Status messages are informational and must not trigger large JSON
    // serializations / DOM layout during fast moving scenes.
    const hdPending = hdEnabled && Boolean(liveHd.assets) &&
      (game.hd_background_pending?.()??false);
    const interval = hdPending ? 180 : hdEnabled ? 750 : 900;
    if (now - lastHdPoll >= interval) {
      lastHdPoll = now;
      liveHd.pulse();
      liveFighters.pulse();
      livePortraits.pulse();
      liveSpecials.pulse();
      liveMaps.pulse();
      liveCards.pulse();
      liveMode7.pulse();
    }
    // On 120 Hz iPhone displays ~half of RAF calls have no emulated
    // frame. Repainting the same 512x478 ImageData wasted bandwidth.
    const drawStart=performance.now();
    if (n) drawFrame();
    if (n) hdSceneTrace.capture(game,lastPresentation);
    const drawMs=performance.now()-drawStart;
    const perf=frameMetrics.add({now,gap:rafGap,tickMs:simulationMs,drawMs,ticks:n});
    if(perf){
      perfHistory.push({frame:game.frame_count(),...perf});
      if(perfHistory.length>30)perfHistory.shift();
      $('frame-perf').textContent=FrameMetrics.describe(perf)+
        '；轉場異常影格已保護 '+transitionArtifactGate.heldFramesTotal;
      // One aggregated JSON report per two seconds, not per frame.
      // Isolate ROM emulation, PPU, layered compositing, and translation.
      if(typeof game.hd_profile_report==='function'){
        const raw=game.hd_profile_report();
        $('hd-phase-perf').textContent=describeHdPhases(raw,liveMaps);
      }
    }
    if (now - lastDiagPoll >= 900) {
      lastDiagPoll = now;
      sampleTranslationDiagnostics();
    }
    if(!countWindow) countWindow=now;
    if(now-countWindow>=1000){
      $('frames').textContent=`${game.frame_count()} 幀`;
      $('fps').textContent=`約 ${(countFrames*1000/(now-countWindow)).toFixed(1)} 模擬幀/秒`;
      $('hd-info').textContent=hdEnabled
        ? (game.hd_active() ? `HD 2x：${game.width()}×${game.height()}` : `HD 已回退 Classic：${game.hd_warning()}`)
        : classicZhEnabled ? 'Classic 繁體中文 1x（低耗能）' : 'Classic 英文原版';
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
  fighterFrameGate.reset();
});
$('mode7-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_mode7_hd_enabled?.($('mode7-hd-toggle').checked);
  mode7FrameGate.reset();
});
$('card-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_card_hd_enabled?.($('card-hd-toggle').checked);
  cardFrameGate.reset();
});
$('map-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_map_hd_enabled?.($('map-hd-toggle').checked);
  mapFrameGate.reset();
});
$('special-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_special_hd_enabled?.($('special-hd-toggle').checked);
  specialFrameGate.reset();
});
$('portrait-hd-toggle').addEventListener('change',()=>{
  if(game)game.set_portrait_hd_enabled?.($('portrait-hd-toggle').checked);
  portraitFrameGate.reset();
});
$('locale-toggle').addEventListener('click',async()=>{
  if(!game)return;
  const button=$('locale-toggle'),next=language==='zh-hk'?'zh-tw':'zh-hk';
  button.disabled=true;
  try {
    const count=game.load_translation(await getTranslation(next));
    language=next;
    try {localStorage.setItem('dbz-hd-language',language);}catch{}
    updateLanguageButton();
    $('translation-count').textContent='已載入 '+count+' 筆'+button.textContent+'譯文';
    translationKey='';currentDialogue=null;
    storyFrameGate.reset();hdFrameGate.reset();portraitFrameGate.reset();
  }catch(error){
    window.alert('語言切換失敗：'+(error?.message||error));
  }finally{button.disabled=false;}
});
updateLanguageButton();
updateRenderModeButton();
// Tap cycles HD 2x -> Classic Chinese 1x -> original English Classic.
// This is a rendering setting only: ROM, SRAM, save states and audio stay put.
$('hd-toggle').addEventListener('click', () => {
  const current=hdEnabled?'hd':classicZhEnabled?'classic-zh':'classic';
  const modes=['hd','classic-zh','classic'];
  hdPreference=modes[(modes.indexOf(current)+1)%modes.length];
  hdEnabled=hdPreference==='hd';
  classicZhEnabled=hdPreference==='classic-zh';
  try{localStorage.setItem('dbz-hd-mode',hdPreference);}catch{}
  updateRenderModeButton();
  $('hd-info').textContent=hdEnabled?'正在初始化 HD 渲染…':
    classicZhEnabled?'Classic 繁體中文 1x（低耗能）':'Classic 英文原版';
  if(game) {
    game.set_hd_mode(hdEnabled);
    game.set_classic_zh_mode(classicZhEnabled);
  }
  storyFrameGate.reset();
  hdFrameGate.reset();
  portraitFrameGate.reset();
  fighterFrameGate.reset();
  specialFrameGate.reset();
  mapFrameGate.reset();
  cardFrameGate.reset();
  mode7FrameGate.reset();
  transitionArtifactGate.reset();
  liveHd.pulse();
  liveFighters.pulse();
  livePortraits.pulse();
  liveSpecials.pulse();
  liveMaps.pulse();
  liveCards.pulse();
  liveMode7.pulse();
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
  // Begin AudioContext recreation BEFORE awaiting anything: Safari requires
  // a real tap for the new output graph, even if the old context says running.
  const button=$('sound');
  button.disabled=true;
  try {
    const rate=await sound.reconnectFromGesture();
    if(game)game.set_audio_rate(rate);
    soundUi(`音訊輸出已重新連接 · ${rate} Hz；如仍無聲，可再按一次`);
  } catch(error) {
    soundUi(`重新連接失敗：${error?.message||error}`);
  } finally {
    button.disabled=false;
  }
});
const stop=()=>{offerManualSoundRecovery=false;hdSceneTrace.reset();perfHistory=[];transientHoldBudget.reset();
lastPresentedFrame=null;lastPresentation=null;translationDiag=[];lastDiagSignature='';storyFrameGate.reset();hdFrameGate.reset();portraitFrameGate.reset();fighterFrameGate.reset();specialFrameGate.reset();mapFrameGate.reset();cardFrameGate.reset();mode7FrameGate.reset();liveHd.detach();liveFighters.detach();livePortraits.detach();liveSpecials.detach();liveMaps.detach();liveCards.detach();liveMode7.detach();running=false;cancelAnimationFrame(raf);sound.reset();sound.mute().catch(console.warn);game=null;held=0;translationKey='';currentDialogue=null;embedded=false;$('translation-card').hidden=true;$('game').hidden=true;$('setup').hidden=false;document.body.classList.remove('playing');$('start').disabled=false;refreshLaunchCover();};
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
  if(document.hidden&&game){
    if(running){running=false;cancelAnimationFrame(raf);}
    held=0;sound.reset();
    offerManualSoundRecovery=Boolean(sound.enabled);
  }else if(!document.hidden&&game){
    if(!running){
      running=true;lastTick=0;accumulator=0;raf=requestAnimationFrame(requestFrame);
    }
    offerManualSoundRecovery=Boolean(sound.enabled);
    soundUi();
    void restoreAudioAfterInterruption();
  }
});
window.addEventListener('pageshow',()=>{
  if(game && !document.hidden){
    offerManualSoundRecovery=Boolean(sound.enabled);
    soundUi();
    void restoreAudioAfterInterruption();
  }
});
$('save').addEventListener('click',()=>{
  if(!game)return;
  const bytes=game.save_state();
  const url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'}));
  const a=document.createElement('a');a.href=url;a.download='dbz-ssd-web.sav';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
});
$('state-file').addEventListener('change',async e=>{
  const file=e.target.files?.[0];
  if(!game||!file)return;
  try {
    game.load_state(new Uint8Array(await file.arrayBuffer()));
    hdSceneTrace.reset();transientHoldBudget.reset();perfHistory=[];
    storyFrameGate.reset();hdFrameGate.reset();portraitFrameGate.reset();
    fighterFrameGate.reset();specialFrameGate.reset();mapFrameGate.reset();
    cardFrameGate.reset();mode7FrameGate.reset();transitionArtifactGate.reset();
    sound.reset();translationKey='';updateTranslation();
    // The native core preserves host audio rate, but mobile file selection
    // may leave AudioContext suspended. Resume and reassert its sample rate.
    const restored=await restoreAudioAfterInterruption();
    gameToast(restored?'讀取進度成功':'讀取進度成功；如無聲請點「恢復聲音」');
  }catch(err){gameToast('讀取進度失敗：'+(err?.message||err));}
  finally {e.target.value='';}
});
// Do not register a Service Worker. Large WASM streaming must bypass it.
// Legacy registrations from v41-v43 are retired before the user selects files
// (see the early bootstrap in index.html).
