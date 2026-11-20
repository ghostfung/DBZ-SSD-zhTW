// Network-first version updates: prevent old Safari scripts with new WASM.
// Offline cache remains, and user ROM/IPS/HD never leave the device.
const CACHE='dbz-ssd-web-shell-v40';
const ASSETS=['./','./index.html','./style.css','./app.mjs','./story-frame-gate.mjs','./hd-frame-gate.mjs','./screen-text.mjs','./hd-live.mjs','./hd-fighter-index.mjs','./hd-fighter-live.mjs','./hd-portrait-index.mjs','./hd-portrait-live.mjs','./hd-portrait-gate.mjs','./hd-special-index.mjs','./hd-special-live.mjs','./hd-map-index.mjs','./hd-map-live.mjs','./hd-map-prefetch.mjs','./hd-card-index.mjs','./hd-card-live.mjs','./hd-mode7-index.mjs','./hd-mode7-live.mjs','./frame-metrics.mjs','./hd-phase-report.mjs','./transition-artifact-gate.mjs','./audio.mjs','./chinese-overlay.mjs','./hd-import-ui.mjs','./hd-local-assets.mjs','./hd-background-index.mjs','./hd-coverage-audit.mjs','./hd-coverage-runtime.mjs','./hd-pack.mjs','./hd-storage.mjs','./sha256-stream.mjs','./rom-check.mjs','./zh-tw.tsv','./zh-tw-page-alignments.tsv','./manifest.webmanifest'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(ASSETS)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{
  const request=event.request;
  if(request.method!=='GET'||new URL(request.url).origin!==self.location.origin)return;
  const path=new URL(request.url).pathname;
  const cacheable=/\.(?:mjs|js|wasm|css|html|tsv|webmanifest)$/.test(path)||path.endsWith('/')||path.includes('/pkg/');
  event.respondWith(fetch(request,{cache:'no-store'}).then(async response=>{
    if(response.ok&&cacheable){
      const cache=await caches.open(CACHE);
      await cache.put(request,response.clone());
    }
    return response;
  }).catch(async error=>{
    const cached=await caches.match(request,{ignoreSearch:true});
    if(cached)return cached;
    throw error;
  }));
});
