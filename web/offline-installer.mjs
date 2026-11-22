// v59 full offline install; user ROM/IPS/HD ZIP/saves remain in local file storage.
export const OFFLINE_CACHE='dbz-ssd-offline-v59';
export const OFFLINE_RELEASE='v59';
export const OFFLINE_WASM_SIZE=18559329;
export const OFFLINE_FILES=Object.freeze(["app.mjs","apple-touch-icon.png","audio.mjs","cheat-preferences.mjs","chinese-overlay.mjs","dbz-title.webp","frame-metrics.mjs","hd-background-index.mjs","hd-card-index.mjs","hd-card-live.mjs","hd-coverage-audit.mjs","hd-coverage-runtime.mjs","hd-fighter-diagnostic.mjs","hd-fighter-index.mjs","hd-fighter-live.mjs","hd-frame-gate.mjs","hd-import-ui.mjs","hd-live.mjs","hd-local-assets.mjs","hd-map-index.mjs","hd-map-live.mjs","hd-map-prefetch.mjs","hd-mode7-index.mjs","hd-mode7-live.mjs","hd-pack.mjs","hd-phase-report.mjs","hd-portrait-gate.mjs","hd-portrait-index.mjs","hd-portrait-live.mjs","hd-presentation-budget.mjs","hd-special-index.mjs","hd-special-live.mjs","hd-storage.mjs","hd-variant-index.mjs","index.html","local-game-files.mjs","manifest.webmanifest","offline-installer.mjs","pkg/snippets/ssd-web-78d3277d0c70cc4b/inline0.js","pkg/ssd_web.js","pwa-icon.svg","rom-check.mjs","save-slots.mjs","screen-text.mjs","sha256-stream.mjs","speed-control.mjs","story-frame-gate.mjs","style.css","transition-artifact-gate.mjs","zh-hk.tsv","zh-tw-page-alignments.tsv","zh-tw.tsv","pkg/ssd_web_bg.wasm"]);
const rootFor=root=>new URL(root??new URL('./',import.meta.url).href);
const asset=(file,root)=>new URL('./'+file,rootFor(root)).href;
const marker=root=>new URL('./__dbz-offline-ready-v59__',rootFor(root)).href;
export async function inspectOfflineCache({storage=globalThis.caches,root}={}){
  if(!storage)return {ready:false,found:0,total:OFFLINE_FILES.length};
  const cache=await storage.open(OFFLINE_CACHE);
  if(!await cache.match(marker(root)))return {ready:false,found:0,total:OFFLINE_FILES.length};
  const items=await Promise.all(OFFLINE_FILES.map(f=>cache.match(asset(f,root),{ignoreSearch:true})));
  const found=items.filter(Boolean).length;
  return {ready:found===OFFLINE_FILES.length,found,total:OFFLINE_FILES.length};
}
export async function loadOfflineWasm(url,{storage=globalThis.caches}={}){
  if(!storage)return null;
  // Direct CacheStorage bytes bypass the broken iOS SW/WASM stream bridge.
  const cache=await storage.open(OFFLINE_CACHE);
  const response=await cache.match(url,{ignoreSearch:true});
  if(!response)return null;
  const bytes=new Uint8Array(await response.arrayBuffer());
  if(bytes.length!==OFFLINE_WASM_SIZE||bytes[0]!==0||bytes[1]!==97||bytes[2]!==115||bytes[3]!==109)return null;
  return bytes;
}
export async function installOfflineFiles({storage=globalThis.caches,root,onProgress=()=>{},online=()=>true}={}){
  if(!storage)throw Error('不支援離線儲存');
  const cache=await storage.open(OFFLINE_CACHE);
  const state=await inspectOfflineCache({storage,root});
  if(state.ready)return state;
  if(!online())throw Error('需要連線完成首次離線安裝');
  await cache.delete(marker(root));
  let done=0;
  // Three concurrent small fetches, with the large WASM always last.
  // Cache.add streams directly to the cache; there is no Response.clone.
  for(let i=0;i<OFFLINE_FILES.length;i+=3){
    await Promise.all(OFFLINE_FILES.slice(i,i+3).map(async file=>{
      const url=asset(file,root);
      if(!await cache.match(url,{ignoreSearch:true})){
        await cache.add(new Request(url,{cache:'reload',credentials:'same-origin'}));
      }
      onProgress(++done,OFFLINE_FILES.length,file);
    }));
  }
  const wasm=await cache.match(asset('pkg/ssd_web_bg.wasm',root),{ignoreSearch:true});
  if(!wasm||(await wasm.blob()).size!==OFFLINE_WASM_SIZE)throw Error('遊戲核心未完整儲存');
  const all=await Promise.all(OFFLINE_FILES.map(f=>cache.match(asset(f,root),{ignoreSearch:true})));
  if(all.some(x=>!x))throw Error('離線資源缺失');
  await cache.put(marker(root),new Response(OFFLINE_RELEASE,{headers:{'Content-Type':'text/plain'}}));
  return {ready:true,found:all.length,total:all.length};
}
function show(message,retry=false){
  const output=document.getElementById('offline-status-message'),button=document.getElementById('offline-retry');
  if(output)output.textContent=message;
  if(button)button.hidden=!retry;
}
let active=null;
export async function prepareOffline(){
  if(active)return active;
  active=(async()=>{
    if(!('serviceWorker' in navigator)||!globalThis.caches){show('此瀏覽器無法安裝離線版');return;}
    try{
      const base=new URL('./',import.meta.url);
      const existing=await navigator.serviceWorker.getRegistration(base.href);
      // Avoid re-registering or network update checks on later launches.
      if(!existing?.active?.scriptURL.includes('offline=v59')){
        if(!navigator.onLine)throw Error('請先連線安裝離線版');
        await navigator.serviceWorker.register('./service-worker.js?offline=v59',{scope:'./',updateViaCache:'none'});
      }
      await navigator.serviceWorker.ready;
      const current=await inspectOfflineCache();
      if(current.ready){show('離線版已安裝 ✓ 飛航模式可重新啟動');return;}
      show('正在下載完整離線程式（包含 18 MB 遊戲核心）…');
      await installOfflineFiles({online:()=>navigator.onLine,onProgress:(done,total,file)=>{
        show('離線安裝 '+Math.round(done/total*100)+'% ('+done+'/'+total+')'+
          (file.endsWith('.wasm')?' · 核心':''));
      }});
      show('離線版已安裝 ✓ 飛航模式可重新啟動');
    }catch(error){show('離線安裝未完成：'+(error?.message||error),true);}
  })();
  try{return await active;}finally{active=null;}
}
if(typeof document!=='undefined'){
  document.getElementById('offline-retry')?.addEventListener('click',()=>{void prepareOffline();});
  void prepareOffline();
}
