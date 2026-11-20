// Network-first version updates: prevent old Safari scripts with new WASM.
// Offline cache remains, and user ROM/IPS/HD never leave the device.
const CACHE='dbz-ssd-web-shell-v43';
const ASSETS=['./','./index.html','./style.css','./app.mjs','./story-frame-gate.mjs','./hd-frame-gate.mjs','./screen-text.mjs','./hd-live.mjs','./hd-fighter-index.mjs','./hd-variant-index.mjs','./hd-fighter-live.mjs','./hd-portrait-index.mjs','./hd-portrait-live.mjs','./hd-portrait-gate.mjs','./hd-special-index.mjs','./hd-special-live.mjs','./hd-map-index.mjs','./hd-map-live.mjs','./hd-map-prefetch.mjs','./hd-card-index.mjs','./hd-card-live.mjs','./hd-mode7-index.mjs','./hd-mode7-live.mjs','./frame-metrics.mjs','./hd-presentation-budget.mjs','./hd-phase-report.mjs','./transition-artifact-gate.mjs','./audio.mjs','./chinese-overlay.mjs','./hd-import-ui.mjs','./hd-local-assets.mjs','./hd-background-index.mjs','./hd-coverage-audit.mjs','./hd-coverage-runtime.mjs','./hd-pack.mjs','./hd-storage.mjs','./sha256-stream.mjs','./rom-check.mjs','./zh-tw.tsv','./zh-tw-page-alignments.tsv','./manifest.webmanifest'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(ASSETS)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{
  const request=event.request;
  if(request.method!=='GET'||new URL(request.url).origin!==self.location.origin)return;
  const path=new URL(request.url).pathname;
  const isWasm=path.endsWith('.wasm');
  const cacheable=/\.(?:mjs|js|css|html|tsv|webmanifest)$/.test(path)||path.endsWith('/')||path.includes('/pkg/');
  event.respondWith(fetch(request,{cache:isWasm?'default':'no-store'}).then(response=>{
    // Critical: NEVER await CacheStorage writes before returning the fetch
    // Response. Safari previously held back WASM streaming behind a ~18 MB
    // cache clone/write and looked like a frozen start button.
    // Versioned WASM URLs use the native browser cache, no duplicate
    // CacheStorage write and no extra 18 MB clone in mobile memory.
    if(response.ok&&cacheable&&!isWasm){
      try{
        const copy=response.clone();
        event.waitUntil(caches.open(CACHE)
          .then(cache=>cache.put(request,copy))
          .catch(()=>{})); // Cache quota may fail; networking still works
      }catch{/* The cache is optional, not a condition for boot. */}
    }
    return response;
  }).catch(async error=>{
    // Never ignore query strings when falling back to cached JS; doing so
    // could pair new HTML with a previous release's old module/WASM.
    const cached=await caches.match(request,{ignoreSearch:false});
    if(cached)return cached;
    throw error;
  }));
});
