import {inspectRom,crc32,crcHex} from './rom-check.mjs';
import {JoypadInput} from './joypad-input.mjs';
import {SLOT_COUNT,listSaveSlots,readSaveSlot,writeSaveSlot,validStateBytes,thumbnailDataUrl,optionalThumbnail} from './save-slots.mjs?ui=v76';
import {createMigrationBundle,readMigrationBundle} from './migration-bundle.mjs';
import {SnesAudio} from './audio.mjs?build=v65';
import {loadOfflineWasm} from './offline-installer.mjs?offline=v76';
import {nextSpeed,speedFramePlan,SPEED_NOTICE_KEY} from './speed-control.mjs?build=v75';
import {saveCheatChoices,restoreCheatChoices} from './cheat-preferences.mjs?ui=v53';
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
const BUILD_ID='dbz-hd-tw-pwa-v73';
const $ = id => document.getElementById(id);
let moduleReady = null, game = null, running = false, raf = 0;
const joypad=new JoypadInput(),pointerSets=[];
function resetControls(){joypad.reset();for(const pointers of pointerSets)pointers.clear();for(const btn of document.querySelectorAll('button[data-bit].pressed'))btn.classList.remove('pressed');}
let cheatPanelOpen = false, cheatNoticeAcknowledged=false;
const CHEAT_NOTICE_KEY='dbz-ssd-cheat-exp-level8-warning-v1';
let gameSpeed=1,speedNoticeOpen=false,speedNoticeShownInSession=false;
let pendingCheatRestore = false;
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
let savedRom=null,savedIps=null,currentRomKey='';
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
  $('locale-toggle').textContent=language==='zh-hk'?'香港粵語':'台灣繁中';
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

// Exactly one speed button: portrait above Start, landscape above D-pad.
// Move its existing DOM node on orientation change, never clone handlers.
const landscapeSpeedPlacement=window.matchMedia('(orientation:landscape)');
function syncSpeedPlacement(){
  const anchor=$(landscapeSpeedPlacement.matches?'speed-landscape-anchor':'speed-portrait-anchor');
  if(anchor&&$('speed-toggle').parentElement!==anchor)anchor.appendChild($('speed-toggle'));
}
landscapeSpeedPlacement.addEventListener?.('change',syncSpeedPlacement);
window.addEventListener('resize',syncSpeedPlacement);
syncSpeedPlacement();
function updateSpeedButton(){
  const button=$('speed-toggle');
  button.textContent='速度 '+gameSpeed+'×';
  button.dataset.speed=String(gameSpeed);
  button.setAttribute('aria-label','目前遊戲速度 '+gameSpeed+' 倍，點擊切換');
  button.title=gameSpeed===1?'遊戲原速，點擊切換至 2×':
    '遊戲倍速 '+gameSpeed+'×（音效同步加速），點擊切換';
  button.setAttribute('aria-pressed',String(gameSpeed!==1));
}
$('speed-toggle').addEventListener('click',()=>{
  if(!game||speedNoticeOpen)return;
  gameSpeed=nextSpeed(gameSpeed);
  sound.setPlaybackRate(gameSpeed);
  lastTick=0;accumulator=0;
  // The button itself shows the selected speed. Do not show the old
  // full-width toast: on portrait iPhone it overlaps this compact control.
  updateSpeedButton();
});
updateSpeedButton();

function showSpeedUpdateNoticeOnce(){
  if(speedNoticeShownInSession)return false;
  try{if(localStorage.getItem(SPEED_NOTICE_KEY)==='1')return false;}catch{}
  speedNoticeOpen=true;resetControls();
  $('speed-update-panel').hidden=false;
  $('speed-update-close').focus({preventScroll:true});
  return true;
}
$('speed-update-close').addEventListener('click',()=>{
  if(!speedNoticeOpen)return;
  speedNoticeShownInSession=true;
  try{localStorage.setItem(SPEED_NOTICE_KEY,'1');}catch{}
  speedNoticeOpen=false;
  $('speed-update-panel').hidden=true;
  $('speed-toggle').focus({preventScroll:true});
  if(game&&!running&&!document.hidden&&!cheatPanelOpen){
    running=true;lastTick=0;accumulator=0;
    raf=requestAnimationFrame(requestFrame);
  }
});
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

document.addEventListener('dbz-offline-update-ready',event=>{
  const version=event.detail?.version||'新版';
  if(game)gameToast(version+' 已經下載完成。請先儲存進度，返回首頁後重新啟動更新。');
});

// First-phase cheat settings are separate browser-local preferences; existing
// ROM, save-state and emulation data formats remain unchanged.
function cheatPreferenceStorage(){
  try{return localStorage;}catch{return null;}
}
function persistActiveCheats(){
  return saveCheatChoices(cheatPreferenceStorage(),JSON.parse(game.cheat_state()));
}
function restoreActiveCheats(){
  const result=restoreCheatChoices(game,cheatPreferenceStorage());
  // On a loaded save in battle, the Rust core intentionally rejects changing
  // cheats. Preserve the preferences and retry only after play returns to a
  // safe state; do not bypass its scripted-battle protection.
  pendingCheatRestore=Boolean(result.error?.includes('戰鬥進行中'));
  return result;
}
// Cheats are owned by the active WASM machine; never inferred from UI state.
function refreshCheatPanel(message='') {
  if(!game) return;
  const state=JSON.parse(game.cheat_state());
  for(const control of document.querySelectorAll('[data-cheat]')){
    const value=state[control.dataset.cheat];
    if(control.type==='checkbox')control.checked=Boolean(value);
    else control.value=String(value);
  }
  $('cheat-feedback').textContent=message||
    (state.active===0?'全部關閉 · 原版遊戲規則':
      '已啟用 '+state.active+' 項 · 戰鬥中不能調整');
}
function openCheatPanel(){
  if(!game||cheatPanelOpen)return;
  cheatPanelOpen=true;
  resetControls();
  if(running){running=false;cancelAnimationFrame(raf);}
  sound.reset();
  if(!cheatNoticeAcknowledged){
    try{cheatNoticeAcknowledged=localStorage.getItem(CHEAT_NOTICE_KEY)==='1';}catch{}
  }
  if(!cheatNoticeAcknowledged){
    $('cheat-warning-panel').hidden=false;
    $('cheat-warning-confirm').focus({preventScroll:true});
    return;
  }
  revealCheatControls();
}
function revealCheatControls(){
  $('cheat-warning-panel').hidden=true;
  $('cheats-panel').hidden=false;
  refreshCheatPanel();
  $('cheat-close').focus({preventScroll:true});
}
function closeCheatPanel(){
  if(!cheatPanelOpen)return;
  cheatPanelOpen=false;
  $('cheat-warning-panel').hidden=true;
  $('cheats-panel').hidden=true;
  if(game&&!document.hidden&&!running&&!speedNoticeOpen){
    running=true;lastTick=0;accumulator=0;
    raf=requestAnimationFrame(requestFrame);
  }
  $('cheat-open').focus({preventScroll:true});
}
$('cheat-warning-confirm').addEventListener('click',()=>{
  if(!cheatPanelOpen||$('cheat-warning-panel').hidden)return;
  cheatNoticeAcknowledged=true;
  try{localStorage.setItem(CHEAT_NOTICE_KEY,'1');}catch{}
  revealCheatControls();
});
$('cheat-warning-cancel').addEventListener('click',closeCheatPanel);
$('cheat-open').addEventListener('click',openCheatPanel);
$('cheat-close').addEventListener('click',closeCheatPanel);
$('cheat-done').addEventListener('click',closeCheatPanel);
for(const control of document.querySelectorAll('[data-cheat]')){
  control.addEventListener('change',()=>{
    if(!game)return;
    try{
      const value=control.type==='checkbox'?String(control.checked):control.value;
      game.set_cheat_option(control.dataset.cheat,value);
      pendingCheatRestore=false;
      const saved=persistActiveCheats();
      refreshCheatPanel(saved?'已套用並儲存在此手機':'已套用，但無法儲存在此瀏覽器');
    }catch(error){
      refreshCheatPanel('操作未套用：'+(error?.message||String(error)));
    }
  });
}
$('cheat-reset').addEventListener('click',()=>{
  if(!game)return;
  try{
    game.set_cheat_option('reset','all');
    pendingCheatRestore=false;
    const saved=persistActiveCheats();
    refreshCheatPanel(saved?'作弊與本機偏好已全部重設':'作弊已關閉，但無法更新本機偏好');
  }catch(error){
    refreshCheatPanel('無法重設：'+(error?.message||String(error)));
  }
});

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
  if(ready)$('setup').classList.remove('managing-files');
  $('setup-edit').hidden=!ready;
}
$('setup-edit').addEventListener('click',()=>{
  $('setup').classList.remove('launch-ready');
  $('setup').classList.add('managing-files');
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
    const mod = await import('./pkg/ssd_web.js?build=v73');
    // Use the wasm-pack loader from the previously working v41 release.
    // Its versioned URL can stream normally, with no manually-created
    // Response passed across WebKit's Service Worker boundary.
    await phase('正在直接下載及編譯 WASM 遊戲核心（約 18 MB）…');
    const wasmUrl=new URL('./pkg/ssd_web_bg.wasm?build=v73',import.meta.url);
    // Read verified, previously installed WASM bytes directly from
    // CacheStorage, never via an intercepting Service Worker Response.
    // Without an offline copy retain the exact v58 direct-network loader.
    const offlineWasm=await loadOfflineWasm(wasmUrl);
    await mod.default({module_or_path:offlineWasm??wasmUrl});
    moduleReady = mod;
    return mod;
  } catch (error) {
    throw Error('WASM 初始化失敗：'+(error?.name==='AbortError'?
      '網路下載超時。請檢查網路後重試。':error?.message||String(error)));
  }
}

$('start').addEventListener('click', async () => {
  if(document.body.classList.contains('offline-preparing'))return;
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
    // Bind slots to the exact player-supplied ROM and IPS. No bytes leave the PWA.
    currentRomKey=crcHex(crc32(rom))+':'+crcHex(crc32(ips));
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
    await phase('正在載入'+(language==='zh-hk'?'香港粵語':'台灣繁中')+'譯文…');
    const translatedRows=game.load_translation(await getTranslation(language));
    $('translation-count').textContent = '已載入 ' + translatedRows + ' 筆'+(language==='zh-hk'?'香港粵語':'台灣繁中')+'譯文';
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
    // Restore after the new WASM game has been constructed, but before its
    // first emulated frame. Do not embed preferences in .sav game states.
    const cheatRestore=restoreActiveCheats();
    if (sound.enabled && sound.state === 'running') game.set_audio_rate(sound.sampleRate);
    soundUi();
    refreshCheatPanel(pendingCheatRestore
      ? '已保留上次的作弊設定；遊戲進入安全階段後自動恢復'
      : cheatRestore.error
      ? '上次作弊設定未能恢復，已使用原版規則：'+cheatRestore.error
      : cheatRestore.found
        ? (cheatRestore.applied>0
          ? '已恢復上次儲存的 '+cheatRestore.applied+' 項作弊設定'
          : '已讀取本機設定：全部關閉')
        : '目前使用原版設定；修改後會自動儲存在此手機');
    $('setup').classList.remove('launching');
    $('setup').hidden = true;
    $('game').hidden = false;
    document.body.classList.add('playing');
    updateRenderModeButton();
    button.textContent=originalLabel;
    resetControls(); running = true; lastTick = 0; accumulator = 0; countWindow = 0; countFrames = 0;
    gameSpeed=1;sound.setPlaybackRate(1);updateSpeedButton();syncSpeedPlacement();
    frameMetrics.reset();perfHistory=[];transientHoldBudget.reset();
    hdSceneTrace.reset();lastPresentedFrame=null;lastPresentation=null;
    averageTickMs=0;
    transitionArtifactGate.reset();
    $('frame-perf').textContent='卡頓監測：正在收集影格耗時…';
    $('hd-phase-perf').textContent='HD 分段耗時：正在採樣…';
    if(showSpeedUpdateNoticeOnce())running=false;
    else raf = requestAnimationFrame(requestFrame);
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
    // Fast-forward the emulation clock, never the RAF clock. The scheduler
    // caps catch-up work to avoid freezing HD scenes on older iPhones.
    const plan=speedFramePlan({
      gapMs:rafGap,accumulatorMs:accumulator,rate:gameSpeed,
      averageTickMs,stepMs:STEP
    });
    accumulator=plan.remainingMs;
    let n = 0;
    const simulationStart=performance.now();
    while(n<plan.ticks){
      const held=joypad.sample();
      game.tick(held);countFrames++;n++;
    }
    const simulationMs=performance.now()-simulationStart;
    if(n){
      const perFrame=simulationMs/n;
      averageTickMs=averageTickMs===0 ? perFrame :
        averageTickMs*.75+perFrame*.25;
    }
    // For save states imported mid-battle, retry at most once per 60
    // emulated frames; no additional work during normal gameplay.
    if(pendingCheatRestore && n && game.frame_count()%60===0){
      const restored=restoreActiveCheats();
      if(!pendingCheatRestore){
        refreshCheatPanel(restored.error
          ? '恢復作弊設定失敗：'+restored.error
          : restored.applied>0?'已在安全階段恢復本機作弊設定':'已載入本機設定');
        if(!restored.error && restored.applied>0)gameToast('已自動恢復作弊設定');
      }
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
    // Keep the bounded HD scene trace for read-only troubleshooting.
    if(n)hdSceneTrace.capture(game,lastPresentation);
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
window.addEventListener('keydown',e=>{
  if(cheatPanelOpen){
    if(e.code==='Escape'){e.preventDefault();closeCheatPanel();}
    return;
  }
  if(!game||!running)return;
  if(keyToBit[e.code]!==undefined){joypad.press(keyToBit[e.code],'key:'+e.code);e.preventDefault();}
});
window.addEventListener('keyup',e=>{
  if(keyToBit[e.code]!==undefined){joypad.release(keyToBit[e.code],'key:'+e.code);if(game&&running)e.preventDefault();}
});
for(const btn of document.querySelectorAll('button[data-bit]')){
  const bit=+btn.dataset.bit, repeat=[0,1,8,9].includes(bit), active=new Set();
  pointerSets.push(active);
  btn.addEventListener('pointerdown',e=>{
    if(!game||!running||e.button!==0)return;
    e.preventDefault();active.add(e.pointerId);
    joypad.press(bit,'pointer:'+e.pointerId,repeat);
    btn.classList.add('pressed');
    try{btn.setPointerCapture(e.pointerId);}catch{}
  });
  const release=e=>{
    active.delete(e.pointerId);
    joypad.release(bit,'pointer:'+e.pointerId,e.type!=='pointerup');
    if(!active.size)btn.classList.remove('pressed');
  };
  btn.addEventListener('pointerup',release);
  btn.addEventListener('pointercancel',release);
  btn.addEventListener('lostpointercapture',release);
  btn.addEventListener('contextmenu',e=>e.preventDefault());
  // If a browser cannot capture a pointer, also release it at window level.
  window.addEventListener('pointerup',release);
  window.addEventListener('pointercancel',release);
}
window.addEventListener('blur',resetControls);
window.addEventListener('pagehide',resetControls);
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
const stop=()=>{speedNoticeOpen=false;$('speed-update-panel').hidden=true;gameSpeed=1;sound.setPlaybackRate(1);updateSpeedButton();pendingCheatRestore=false;cheatPanelOpen=false;$('cheat-warning-panel').hidden=true;$('cheats-panel').hidden=true;offerManualSoundRecovery=false;hdSceneTrace.reset();perfHistory=[];transientHoldBudget.reset();
lastPresentedFrame=null;lastPresentation=null;translationDiag=[];lastDiagSignature='';storyFrameGate.reset();hdFrameGate.reset();portraitFrameGate.reset();fighterFrameGate.reset();specialFrameGate.reset();mapFrameGate.reset();cardFrameGate.reset();mode7FrameGate.reset();liveHd.detach();liveFighters.detach();livePortraits.detach();liveSpecials.detach();liveMaps.detach();liveCards.detach();liveMode7.detach();running=false;cancelAnimationFrame(raf);sound.reset();sound.mute().catch(console.warn);game=null;resetControls();translationKey='';currentDialogue=null;embedded=false;$('translation-card').hidden=true;$('game').hidden=true;$('setup').hidden=false;document.body.classList.remove('playing');$('start').disabled=false;refreshLaunchCover();document.dispatchEvent(new Event('dbz-return-home'));};
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
    resetControls();sound.reset();
    offerManualSoundRecovery=Boolean(sound.enabled);
  }else if(!document.hidden&&game){
    if(!running&&!cheatPanelOpen&&!speedNoticeOpen&&!slotMode&&!backupOpen){
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
// v54: all ten slots are stored privately in IndexedDB. ROM, IPS, HD images
// and save data are never sent to any server. The .sav format stays unchanged.
let slotMode=null,slotSelection=null,slotBusy=false,backupOpen=false;
let slotOverwrite=null,slotListReady=false;
let currentSlotRecords=new Map(),slotPickerGeneration=0;

function pauseForProgressMenu(){
  resetControls();
  if(running){running=false;cancelAnimationFrame(raf);}
  sound.reset();
}
function resumeFromProgressMenu(){
  if(game&&!document.hidden&&!running&&!cheatPanelOpen&&!slotMode&&!backupOpen&&!speedNoticeOpen){
    running=true;lastTick=0;accumulator=0;
    raf=requestAnimationFrame(requestFrame);
  }
}
function clearSlotThumbUrls(){
  // Invalidate pending asynchronous preview reads when the picker closes.
  slotPickerGeneration++;
  $('save-slots-list').replaceChildren();
}
function slotButtonLabel(){
  if(slotMode==='load')return '讀取 Slot '+String(slotSelection??'--').padStart(2,'0');
  if(slotOverwrite!==null&&slotOverwrite===slotSelection)return '確認覆蓋 Slot '+String(slotSelection).padStart(2,'0');
  return '儲存至 Slot '+String(slotSelection??'--').padStart(2,'0');
}
function slotAvailable(slot){
  const record=currentSlotRecords.get(slot);
  return slotMode!=='load'||Boolean(record&&!record.corrupt&&record.romKey===currentRomKey);
}
function updateSlotChoice(){
  for(const button of $('save-slots-list').querySelectorAll('[data-slot]'))
    button.setAttribute('aria-pressed',String(Number(button.dataset.slot)===slotSelection));
  $('save-slots-confirm').disabled=slotBusy||!slotListReady||!slotSelection||!slotAvailable(slotSelection);
  $('save-slots-close').disabled=slotBusy;
  $('save-slots-cancel').disabled=slotBusy;
  $('save-slots-confirm').textContent=slotButtonLabel();
}
async function refreshSlotPicker(){
  clearSlotThumbUrls();currentSlotRecords=new Map();slotListReady=false;slotSelection=null;slotOverwrite=null;
  $('save-slots-retry').hidden=true;
  const generation=slotPickerGeneration;
  const list=$('save-slots-list');list.replaceChildren();
  $('save-slots-feedback').textContent='正在讀取本機存檔…';
  $('save-slots-confirm').disabled=true;
  let records;
  try { records=await listSaveSlots(); }
  catch(error){
    if(!slotMode||generation!==slotPickerGeneration)return;
    $('save-slots-feedback').textContent='本機存檔無法使用：'+(error?.message||error)+'。請重試，不要清除網站資料。';
    $('save-slots-retry').hidden=false;
    return;
  }
  if(!slotMode||generation!==slotPickerGeneration)return;
  for(const record of records)currentSlotRecords.set(record.slot,record);
  for(let slot=1;slot<=SLOT_COUNT;slot++){
    const record=currentSlotRecords.get(slot);
    const incompatible=record&&!record.corrupt&&record.romKey!==currentRomKey;
    const button=document.createElement('button');
    button.type='button';button.className='save-slot';
    button.dataset.slot=String(slot);
    button.setAttribute('aria-pressed',String(slot===slotSelection));
    button.disabled=slotMode==='load'&&(!record||record.corrupt||incompatible);
    const preview=record?.thumbnail instanceof Blob&&record.thumbnail.size
      ?document.createElement('img'):document.createElement('span');
    if(preview.tagName==='IMG'){
      preview.alt='存檔 '+slot+' 的遊戲畫面';preview.className='save-slot-thumb';
      const unavailable=()=>{
        if(generation!==slotPickerGeneration)return;
        const fallback=document.createElement('span');
        fallback.className='save-slot-thumb save-slot-empty';
        fallback.textContent='無法預覽';preview.replaceWith(fallback);
      };
      preview.addEventListener('error',unavailable,{once:true});
      void thumbnailDataUrl(record.thumbnail).then(data=>{
        if(generation!==slotPickerGeneration)return;
        if(data)preview.src=data;else unavailable();
      }).catch(unavailable);
    }else{
      preview.className='save-slot-thumb save-slot-empty';
      preview.textContent=record?'無法預覽':'空白';
    }
    const meta=document.createElement('span');meta.className='save-slot-meta';
    const title=document.createElement('strong');
    title.textContent='Slot '+String(slot).padStart(2,'0');
    const line=document.createElement('span');
    line.textContent=record?.corrupt?'進度損壞，不可讀取'
      :record?(incompatible?'不同 ROM／IPS 的進度 · ' : '')+
        new Date(record.savedAt).toLocaleString('zh-TW',{hour12:false})
      :'尚未儲存';
    meta.append(title,line);button.append(preview,meta);
    button.addEventListener('click',()=>{
      if(slotBusy||button.disabled)return;
      slotSelection=slot;slotOverwrite=null;
      $('save-slots-feedback').textContent=slotMode==='load'?'請確認要讀取的欄位。':'請確認要儲存的欄位。';
      updateSlotChoice();
    });
    list.append(button);
  }
  slotListReady=true;
  $('save-slots-feedback').textContent=slotMode==='load'
    ?'請選擇要讀取的進度；不同 ROM／IPS 的欄位不能讀取。'
    :'請選擇儲存欄位。已有進度的欄位會要求確認覆蓋。';
  updateSlotChoice();
}
async function openSlotPicker(mode){
  if(!game||cheatPanelOpen||speedNoticeOpen||(!['save','load'].includes(mode)))return;
  if(slotMode){
    $('save-slots-panel').hidden=false;
    return;
  }
  if(backupOpen){backupOpen=false;$('backup-panel').hidden=true;}
  pauseForProgressMenu();
  slotMode=mode;slotSelection=null;slotBusy=false;slotOverwrite=null;slotListReady=false;
  $('save-slots-title').textContent=
    mode==='save'?'儲存遊戲':'讀取遊戲';
  $('save-slots-hint').textContent='10 個本機進度欄位';
  $('save-slots-panel').hidden=false;
  try{$('save-slots-close').focus({preventScroll:true});}catch{}
  await refreshSlotPicker();
}
function closeSlotPicker(){
  if(!slotMode||slotBusy)return;
  slotMode=null;slotSelection=null;slotOverwrite=null;slotListReady=false;
  $('save-slots-panel').hidden=true;clearSlotThumbUrls();
  $('save').focus({preventScroll:true});
  resumeFromProgressMenu();
}
function openBackupPanel(){
  if(!game||backupOpen||slotMode||cheatPanelOpen)return;
  pauseForProgressMenu();backupOpen=true;
  $('backup-feedback').textContent='';
  $('backup-panel').hidden=false;
  $('backup-close').focus({preventScroll:true});
}
function closeBackupPanel(){
  if(!backupOpen)return;
  backupOpen=false;$('backup-panel').hidden=true;
  $('backup-open').focus({preventScroll:true});
  resumeFromProgressMenu();
}
async function captureSaveThumbnail(){
  const source=$('screen');
  if(!source.width||!source.height)return null;
  const thumb=document.createElement('canvas');
  thumb.width=160;thumb.height=Math.max(1,Math.round(160*source.height/source.width));
  const ctx2=thumb.getContext('2d',{alpha:false});
  if(!ctx2)return null;
  ctx2.imageSmoothingEnabled=Boolean(game?.hd_active?.());
  ctx2.drawImage(source,0,0,thumb.width,thumb.height);
  if(typeof thumb.toBlob!=='function')return null;
  return new Promise(resolve=>thumb.toBlob(resolve,'image/png'));
}
async function applyLoadedSave(bytes){
  validStateBytes(bytes);
  const previous=game.save_state();
  try{game.load_state(bytes);}
  catch(error){
    // An invalid backup must not destroy the in-memory game in progress.
    try{
      game.load_state(previous);
      // load_state always clears runtime cheat memory; restore the player's
      // on-device preferences even when a corrupt .sav was rejected.
      restoreActiveCheats();
    }catch(restoreError){
      console.error('無法恢復匯入前遊戲狀態',restoreError);
    }
    throw error;
  }
  const importedCheats=restoreActiveCheats();
  refreshCheatPanel(pendingCheatRestore
    ?'進度已讀取；戰鬥中暫不修改，回到安全階段後自動恢復作弊'
    :importedCheats.error
      ?'讀取進度成功，但作弊恢復失敗：'+importedCheats.error
      :importedCheats.applied>0
        ?'讀取進度成功，已恢復本機作弊設定'
        :'讀取進度成功；作弊維持關閉');
  hdSceneTrace.reset();transientHoldBudget.reset();perfHistory=[];
  storyFrameGate.reset();hdFrameGate.reset();portraitFrameGate.reset();
  fighterFrameGate.reset();specialFrameGate.reset();mapFrameGate.reset();
  cardFrameGate.reset();mode7FrameGate.reset();transitionArtifactGate.reset();
  sound.reset();translationKey='';updateTranslation();
  // A save restores emulation state, not the iPhone AudioContext's permissions.
  await restoreAudioAfterInterruption();
}
function slotPickerError(error){
  if(slotMode){
    slotListReady=false;slotBusy=false;slotOverwrite=null;
    $('save-slots-panel').hidden=false;
    $('save-slots-feedback').textContent='存檔選單錯誤：'+(error?.message||error)+'。請重試。';
    $('save-slots-retry').hidden=false;updateSlotChoice();
  }else gameToast('無法開啟存檔選單：'+(error?.message||error));
}
$('save').addEventListener('click',()=>{void openSlotPicker('save').catch(slotPickerError);});
$('load').addEventListener('click',()=>{void openSlotPicker('load').catch(slotPickerError);});
$('save-slots-retry').addEventListener('click',()=>{if(slotMode&&!slotBusy)void refreshSlotPicker().catch(slotPickerError);});
$('backup-open').addEventListener('click',openBackupPanel);
$('backup-close').addEventListener('click',closeBackupPanel);
$('save-slots-close').addEventListener('click',closeSlotPicker);
$('save-slots-cancel').addEventListener('click',closeSlotPicker);
// Local-only migration archive. The pre-existing .sav options remain usable.
$('backup-bundle-export').addEventListener('click',async()=>{
  const output=$('backup-feedback');
  try{
    const slots=(await listSaveSlots()).filter(record=>!record.corrupt);
    if(!slots.length)throw Error('尚未有已儲存的欄位，請先儲存遊戲');
    const {blob,count}=createMigrationBundle(slots);
    const url=URL.createObjectURL(blob),link=document.createElement('a');
    link.href=url;link.download='dbz-ssd-all-slots.dbzbackup';link.click();
    setTimeout(()=>URL.revokeObjectURL(url),5000);
    output.textContent='已下載 '+count+' 格存檔；請妥善保存備份檔。';
  }catch(error){output.textContent='完整備份失敗：'+(error?.message||error);}
});
$('backup-bundle-import').addEventListener('change',async event=>{
  const file=event.target.files?.[0],output=$('backup-feedback');
  event.target.value='';
  if(!file||!game)return;
  try{
    const records=await readMigrationBundle(file);
    if(records.some(record=>record.romKey!==currentRomKey))
      throw Error('備份 ROM／IPS 與目前遊戲不一致，請先匯入原本的 ROM／IPS');
    const existing=await listSaveSlots();
    const used=new Set(existing.filter(r=>!r.corrupt).map(r=>r.slot));
    const overwrite=records.filter(r=>used.has(r.slot)).map(r=>r.slot);
    if(overwrite.length&&!window.confirm(
      '將覆蓋本機 Slot '+overwrite.join('、')+'，確定還原備份嗎？'))return;
    let count=0;
    for(const record of records){
      const bytes=validStateBytes(new Uint8Array(await record.state.arrayBuffer()));
      await writeSaveSlot(record.slot,bytes,{
        romKey:currentRomKey,savedAt:record.savedAt,thumbnail:record.thumbnail
      });
      output.textContent='正在還原 '+(++count)+'/'+records.length+' 格…';
    }
    output.textContent='已還原 '+count+' 格存檔。請返回遊戲並點「讀取遊戲」核對。';
  }catch(error){
    output.textContent='完整備份匯入失敗：'+(error?.message||error)+
      '。如果已部分匯入，可用原備份檔再試。';
  }
});

$('save-slots-confirm').addEventListener('click',async()=>{
  const slot=slotSelection,mode=slotMode;
  if(!game||!mode||!slot||slotBusy||!slotListReady||!slotAvailable(slot))return;
  const previous=currentSlotRecords.get(slot);
  if(mode!=='load'&&previous&&slotOverwrite!==slot){
    slotOverwrite=slot;
    $('save-slots-feedback').textContent='Slot '+String(slot).padStart(2,'0')+' 已有進度。再次點「確認覆蓋」才會寫入，或選擇其他欄位。';
    updateSlotChoice();return;
  }
  slotOverwrite=null;
  slotBusy=true;updateSlotChoice();
  $('save-slots-feedback').textContent='正在'+(mode==='load'?'讀取':'儲存')+'進度…';
  try{
    if(mode==='load'){
      const record=await readSaveSlot(slot);
      if(!record||record.romKey!==currentRomKey)throw Error('進度不存在或 ROM／IPS 不相容');
      await applyLoadedSave(new Uint8Array(await record.state.arrayBuffer()));
      slotBusy=false;closeSlotPicker();gameToast('已讀取 Slot '+String(slot).padStart(2,'0'));
    }else{
      const state=validStateBytes(game.save_state());
      $('save-slots-feedback').textContent='正在準備存檔縮圖…';
      const thumb=await optionalThumbnail(captureSaveThumbnail);
      $('save-slots-feedback').textContent='正在寫入本機存檔…';
      await writeSaveSlot(slot,state,{romKey:currentRomKey,thumbnail:thumb});
      slotBusy=false;closeSlotPicker();gameToast('已儲存至 Slot '+String(slot).padStart(2,'0'));
    }
  }catch(error){
    slotOverwrite=null;slotListReady=false;
    $('save-slots-feedback').textContent='操作未完成：'+(error?.name&&error.name!=='Error'?error.name+'：':'')+(error?.message||error)+'。請重新讀取欄位核對；不要清除網站資料。';
    $('save-slots-retry').hidden=false;
  }finally{
    slotBusy=false;
    if(slotMode)updateSlotChoice();
  }
});
// Do not register a Service Worker. Large WASM streaming must bypass it.
// Legacy registrations from v41-v43 are retired before the user selects files
// (see the early bootstrap in index.html).
