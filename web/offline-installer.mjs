// Offline shell installer / low-request daily release updater. No ROM, HD ZIP,
// IPS or saved games are stored here: existing private OPFS/IndexedDB stays intact.
export const OFFLINE_RELEASE='v60';
export const OFFLINE_CACHE='dbz-ssd-offline-v60';
export const OFFLINE_CONTROL='dbz-ssd-offline-control-v1';
export const OFFLINE_WASM_SIZE=18559329;
export const OFFLINE_INTERVAL_MS=24*60*60*1000;
export const OFFLINE_FILES=Object.freeze(["app.mjs","apple-touch-icon.png","audio.mjs","cheat-preferences.mjs","chinese-overlay.mjs","dbz-title.webp","frame-metrics.mjs","hd-background-index.mjs","hd-card-index.mjs","hd-card-live.mjs","hd-coverage-audit.mjs","hd-coverage-runtime.mjs","hd-fighter-diagnostic.mjs","hd-fighter-index.mjs","hd-fighter-live.mjs","hd-frame-gate.mjs","hd-import-ui.mjs","hd-live.mjs","hd-local-assets.mjs","hd-map-index.mjs","hd-map-live.mjs","hd-map-prefetch.mjs","hd-mode7-index.mjs","hd-mode7-live.mjs","hd-pack.mjs","hd-phase-report.mjs","hd-portrait-gate.mjs","hd-portrait-index.mjs","hd-portrait-live.mjs","hd-presentation-budget.mjs","hd-special-index.mjs","hd-special-live.mjs","hd-storage.mjs","hd-variant-index.mjs","index.html","local-game-files.mjs","manifest.webmanifest","offline-installer.mjs","offline-release.json","pkg/snippets/ssd-web-78d3277d0c70cc4b/inline0.js","pkg/ssd_web.js","pwa-icon.svg","rom-check.mjs","save-slots.mjs","screen-text.mjs","sha256-stream.mjs","speed-control.mjs","story-frame-gate.mjs","style.css","transition-artifact-gate.mjs","zh-hk.tsv","zh-tw-page-alignments.tsv","zh-tw.tsv","pkg/ssd_web_bg.wasm"]);
const CURRENT=Object.freeze({version:OFFLINE_RELEASE,wasmSize:OFFLINE_WASM_SIZE,files:OFFLINE_FILES});
const ACTIVE='__dbz-offline-active__';
const PENDING='__dbz-offline-pending__';
const LAST_CHECK='__dbz-offline-last-check__';
const READY='__dbz-offline-ready__';
const VERSION_FORMAT=/^v[1-9][0-9]*(?:\\.[0-9]+){0,2}$/;
const PATH_FORMAT=/^[a-zA-Z0-9][a-zA-Z0-9._\\/-]*$/;
const rootFor=root=>new URL(root??new URL('./',import.meta.url).href);
const asset=(file,root)=>new URL('./'+file,rootFor(root)).href;
const keyUrl=(key,root)=>new URL('./'+key,rootFor(root)).href;
const cacheFor=release=>'dbz-ssd-offline-'+release.version;
function normalized(release){
  if(!release||typeof release!=='object'||!VERSION_FORMAT.test(release.version))
    throw Error('更新版本編號無效');
  const files=release.files;
  if(!Array.isArray(files)||files.length<10||files.length>120||
     new Set(files).size!==files.length||
     files.some(file=>typeof file!=='string'||!PATH_FORMAT.test(file)||
       file.includes('..')||file.startsWith('/')||file.includes('//')))
    throw Error('離線更新資源清單無效');
  for(const mandatory of ['index.html','app.mjs','offline-installer.mjs',
    'pkg/ssd_web.js','pkg/ssd_web_bg.wasm','manifest.webmanifest'])
    if(!files.includes(mandatory))throw Error('必要資源缺失：'+mandatory);
  if(!Number.isInteger(release.wasmSize)||release.wasmSize<1024||
      release.wasmSize>128*1024*1024)throw Error('WASM 長度無效');
  return {version:release.version,wasmSize:release.wasmSize,files:[...files]};
}
function versionParts(version){return version.slice(1).split('.').map(Number);}
export function newerVersion(remote,current){
  if(!VERSION_FORMAT.test(remote)||!VERSION_FORMAT.test(current))return false;
  const a=versionParts(remote),b=versionParts(current);
  for(let i=0;i<3;i++){
    const diff=(a[i]||0)-(b[i]||0);
    if(diff)return diff>0;
  }
  return false;
}
async function control(storage=globalThis.caches){return storage.open(OFFLINE_CONTROL);}
async function readControl(key,{storage=globalThis.caches,root}={}){
  if(!storage)return null;
  const response=await (await control(storage)).match(keyUrl(key,root));
  if(!response)return null;
  try{return JSON.parse(await response.text());}catch{return null;}
}
async function writeControl(key,value,{storage=globalThis.caches,root}={}){
  const c=await control(storage);
  await c.put(keyUrl(key,root),new Response(JSON.stringify(value),{
    headers:{'Content-Type':'application/json'}
  }));
}
async function deleteControl(key,{storage=globalThis.caches,root}={}){
  return (await control(storage)).delete(keyUrl(key,root));
}
export async function getActiveRelease(options={}){
  const entry=await readControl(ACTIVE,options);
  try{return normalized(entry??CURRENT);}catch{return normalized(CURRENT);}
}
export async function getPendingRelease(options={}){
  const entry=await readControl(PENDING,options);
  try{return entry?normalized(entry):null;}catch{return null;}
}
export async function inspectOfflineCache({
  storage=globalThis.caches,root,release
}={}){
  const expected=normalized(release??await getActiveRelease({storage,root}));
  if(!storage)return {ready:false,found:0,total:expected.files.length};
  const c=await storage.open(cacheFor(expected));
  const mark=await c.match(keyUrl(READY,root));
  if(!mark)return {ready:false,found:0,total:expected.files.length};
  let text='';
  try{text=await mark.text();}catch{return {ready:false,found:0,total:expected.files.length};}
  if(text!==expected.version)return {ready:false,found:0,total:expected.files.length};
  const items=await Promise.all(expected.files.map(f=>c.match(asset(f,root),{ignoreSearch:true})));
  const found=items.filter(Boolean).length;
  return {ready:found===expected.files.length,found,total:expected.files.length};
}
async function checkedWasm(cache,release,root){
  const response=await cache.match(asset('pkg/ssd_web_bg.wasm',root),{ignoreSearch:true});
  if(!response)return false;
  if((await response.blob()).size!==release.wasmSize)return false;
  const head=new Uint8Array(await response.slice?.(0,4)?.arrayBuffer?.()??[]);
  // Response.slice is not universally available. Size and magic are
  // validated again when the WASM is loaded for actual use.
  return true;
}
export async function loadOfflineWasm(url,{storage=globalThis.caches,root}={}){
  if(!storage)return null;
  const release=await getActiveRelease({storage,root});
  const response=await (await storage.open(cacheFor(release)))
    .match(url,{ignoreSearch:true});
  if(!response)return null;
  const bytes=new Uint8Array(await response.arrayBuffer());
  if(bytes.length!==release.wasmSize||
     bytes[0]!==0||bytes[1]!==97||bytes[2]!==115||bytes[3]!==109)return null;
  return bytes;
}
export async function installOfflineFiles({
  storage=globalThis.caches,root,release=CURRENT,onProgress=()=>{},online=()=>true
}={}){
  if(!storage)throw Error('此瀏覽器不支援離線儲存');
  const expected=normalized(release),name=cacheFor(expected);
  const state=await inspectOfflineCache({storage,root,release:expected});
  if(state.ready)return state;
  if(!online())throw Error('需要連線完成首次下載');
  const cache=await storage.open(name);
  await cache.delete(keyUrl(READY,root));
  let done=0;
  // Avoid 18 MiB JS memory spikes: Cache.add writes its network Response
  // directly to CacheStorage, WASM is always requested in the final batch.
  for(let i=0;i<expected.files.length;i+=3){
    await Promise.all(expected.files.slice(i,i+3).map(async file=>{
      const url=asset(file,root);
      if(!await cache.match(url,{ignoreSearch:true})){
        await cache.add(new Request(url,{cache:'reload',credentials:'same-origin'}));
      }
      onProgress(++done,expected.files.length,file);
    }));
  }
  if(!await checkedWasm(cache,expected,root))
    throw Error('遊戲核心離線儲存不完整');
  const all=await Promise.all(expected.files.map(f=>cache.match(asset(f,root),{ignoreSearch:true})));
  if(all.some(x=>!x))throw Error('下載資源不完整');
  await cache.put(keyUrl(READY,root),new Response(expected.version));
  return {ready:true,found:all.length,total:all.length};
}
export async function markInitialRelease({storage=globalThis.caches,root}={}){
  const active=await readControl(ACTIVE,{storage,root});
  if(active)return getActiveRelease({storage,root});
  const state=await inspectOfflineCache({storage,root,release:CURRENT});
  if(!state.ready)throw Error('離線程式尚未完整安裝');
  await writeControl(ACTIVE,CURRENT,{storage,root});
  return normalized(CURRENT);
}
export async function stageOfflineUpdate(release,{
  storage=globalThis.caches,root,onProgress=()=>{},online=()=>true
}={}){
  const next=normalized(release);
  const active=await getActiveRelease({storage,root});
  if(!newerVersion(next.version,active.version))return {newer:false,ready:false};
  const state=await installOfflineFiles({storage,root,release:next,onProgress,online});
  if(!state.ready)throw Error('離線更新未完整下載');
  // Never change active files while a game is running.
  await writeControl(PENDING,next,{storage,root});
  return {newer:true,ready:true,version:next.version};
}
export async function activatePendingUpdate({storage=globalThis.caches,root}={}){
  const pending=await getPendingRelease({storage,root});
  if(!pending)throw Error('沒有待套用的新版');
  const state=await inspectOfflineCache({storage,root,release:pending});
  if(!state.ready)throw Error('新版本資源不足，不會切換');
  const active=await getActiveRelease({storage,root});
  if(!newerVersion(pending.version,active.version))
    throw Error('新版本編號必須高於現有版本');
  await writeControl(ACTIVE,pending,{storage,root});
  await deleteControl(PENDING,{storage,root});
  return pending.version;
}
export async function checkReleaseOnceDaily({
  storage=globalThis.caches,root,now=()=>Date.now(),
  online=()=>typeof navigator==='undefined'?true:navigator.onLine,
  fetcher=globalThis.fetch,force=false,onProgress=()=>{}
}={}){
  if(!storage||!online())return {checked:false,reason:'offline'};
  const last=await readControl(LAST_CHECK,{storage,root});
  const time=now();
  if(!force&&typeof last==='number'&&time>=last&&time-last<OFFLINE_INTERVAL_MS)
    return {checked:false,reason:'interval'};
  // Record before request, including failures, to cap automatic CDN checks.
  await writeControl(LAST_CHECK,time,{storage,root});
  let response;
  try{
    response=await fetcher(asset('offline-release.json',root),{cache:'no-store'});
    if(!response.ok)throw Error('HTTP '+response.status);
    const remote=normalized(await response.json());
    const active=await getActiveRelease({storage,root});
    if(!newerVersion(remote.version,active.version))
      return {checked:true,newer:false,version:active.version};
    const staged=await stageOfflineUpdate(remote,{storage,root,online,onProgress});
    return {checked:true,...staged};
  }catch(error){return {checked:true,error:error?.message||String(error)};}
}
function showStatus(message,{retry=false,apply=false,check=false}={}){
  const text=document.getElementById('offline-status-message');
  if(text)text.textContent=message;
  for(const [id,visible] of [
    ['offline-retry',retry],['offline-update-apply',apply],['offline-check',check]
  ]){
    const button=document.getElementById(id);
    if(button)button.hidden=!visible;
  }
}
let running=null;
export async function prepareOffline(){
  if(running)return running;
  running=(async()=>{
    if(!('serviceWorker' in navigator)||!globalThis.caches){
      showStatus('此瀏覽器不支援完整離線安裝');return;
    }
    try{
      const root=new URL('./',import.meta.url);
      const installed=await navigator.serviceWorker.getRegistration(root.href);
      if(!installed?.active?.scriptURL.includes('/service-worker.js')){
        if(!navigator.onLine)throw Error('初次安裝請先連上網絡');
        await navigator.serviceWorker.register('./service-worker.js?offline=v60',
          {scope:'./',updateViaCache:'none'});
      }
      await navigator.serviceWorker.ready;
      let active=await getActiveRelease();
      let state=await inspectOfflineCache({release:active});
      if(!state.ready){
        showStatus('正在保存完整離線程式與遊戲核心…');
        await installOfflineFiles({
          release:active,online:()=>navigator.onLine,
          onProgress:(done,total,file)=>showStatus(
            '離線安裝 '+Math.round(done/total*100)+'%（'+done+'/'+total+'）'+
            (file.endsWith('.wasm')?' · 核心':''))
        });
        active=await markInitialRelease();
        state=await inspectOfflineCache({release:active});
      }else{
        await markInitialRelease();
      }
      if(!state.ready)throw Error('離線程式未完成安裝');
      const pending=await getPendingRelease();
      if(pending){
        const ready=await inspectOfflineCache({release:pending});
        if(ready.ready){
          showStatus('新版 '+pending.version+' 已完整下載，儲存進度後重新啟動即可更新',
            {apply:true,check:true});
          return;
        }
      }
      showStatus('離線版已安裝 ✓ 可斷網遊玩',{check:true});
      const check=await checkReleaseOnceDaily({
        online:()=>navigator.onLine,
        onProgress:(done,total,file)=>showStatus(
          '新版本下載中 '+Math.round(done/total*100)+'%（'+done+'/'+total+'）'+
          (file.endsWith('.wasm')?' · 核心':''))
      });
      if(check.ready){
        showStatus('新版 '+check.version+' 已下載，儲存進度後按「重新啟動更新」',
          {apply:true,check:true});
        document.dispatchEvent(new CustomEvent('dbz-offline-update-ready',
          {detail:{version:check.version}}));
      }else if(check.error){
        showStatus('離線可正常遊玩；暫時無法檢查更新',{check:true});
      }else{
        showStatus('離線版已安裝 ✓ 可斷網遊玩',{check:true});
      }
    }catch(error){
      showStatus('離線安裝未完成：'+(error?.message||error)+'，請連線重試',
        {retry:true});
    }
  })();
  try{return await running;}finally{running=null;}
}
if(typeof document!=='undefined'){
  document.getElementById('offline-retry')?.addEventListener('click',
    ()=>{void prepareOffline();});
  document.getElementById('offline-check')?.addEventListener('click',async()=>{
    showStatus('正在檢查新版…');
    const result=await checkReleaseOnceDaily({
      force:true,online:()=>navigator.onLine,
      onProgress:(done,total)=>showStatus(
        '新版本下載中 '+Math.round(done/total*100)+'%（'+done+'/'+total+'）')
    });
    if(result.ready)showStatus('新版 '+result.version+' 已下載，按「重新啟動更新」',
      {apply:true,check:true});
    else if(result.error)showStatus('無法檢查更新；離線版仍可遊玩',{check:true});
    else showStatus('目前已是最新版本，可繼續離線遊玩',{check:true});
  });
  document.getElementById('offline-update-apply')?.addEventListener('click',async()=>{
    if(document.body.classList.contains('playing')){
      showStatus('請先儲存遊戲並返回首頁，再重新啟動更新',{apply:true,check:true});
      return;
    }
    try{
      showStatus('正在確認完整更新…');
      await activatePendingUpdate();
      window.location.reload();
    }catch(error){
      showStatus('更新未切換：'+(error?.message||error),{retry:true,check:true});
    }
  });
  void prepareOffline();
}
