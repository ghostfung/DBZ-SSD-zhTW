// Cache app shell and compiled wasm; user-provided ROM/IPS is never cached or uploaded.
const CACHE = 'dbz-ssd-web-shell-v39';
const ASSETS = ['./','./index.html','./style.css','./app.mjs','./story-frame-gate.mjs','./hd-frame-gate.mjs','./screen-text.mjs','./hd-live.mjs','./hd-fighter-index.mjs','./hd-fighter-live.mjs','./hd-portrait-index.mjs','./hd-portrait-live.mjs','./hd-portrait-gate.mjs','./hd-special-index.mjs','./hd-special-live.mjs','./hd-map-index.mjs','./hd-map-live.mjs','./hd-map-prefetch.mjs','./hd-card-index.mjs','./hd-card-live.mjs','./hd-mode7-index.mjs','./hd-mode7-live.mjs','./frame-metrics.mjs','./hd-phase-report.mjs','./transition-artifact-gate.mjs','./audio.mjs','./chinese-overlay.mjs','./hd-import-ui.mjs','./hd-local-assets.mjs','./hd-background-index.mjs','./hd-coverage-audit.mjs','./hd-coverage-runtime.mjs','./hd-pack.mjs','./hd-storage.mjs','./sha256-stream.mjs','./rom-check.mjs','./zh-tw.tsv','./zh-tw-page-alignments.tsv','./manifest.webmanifest'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(ASSETS)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{
  const {request}=event;
  if(request.method!=='GET'||new URL(request.url).origin!==self.location.origin)return;
  // Compiled WASM changes after the source deployment. Never serve an old
  // cached runtime to a new JS bridge (Safari otherwise keeps a stale API).
  const packageRequest=new URL(request.url).pathname.includes('/pkg/');
  if(packageRequest){
    event.respondWith(fetch(request).then(async response=>{
      if(response.ok){const cache=await caches.open(CACHE);await cache.put(request,response.clone());}
      return response;
    }).catch(async error=>{
      const cached=await caches.match(request);
      if(cached)return cached;
      throw error;
    }));
    return;
  }
  event.respondWith(caches.match(request).then(async cached=>{
    if(cached)return cached;
    const response=await fetch(request);
    if(response.ok && (request.url.endsWith('.wasm')||request.url.includes('/pkg/'))){
      const cache=await caches.open(CACHE);await cache.put(request,response.clone());
    }
    return response;
  }));
});
