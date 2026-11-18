// Cache app shell and compiled wasm; user-provided ROM/IPS is never cached or uploaded.
const CACHE = 'dbz-ssd-web-shell-v8';
const ASSETS = ['./','./index.html','./style.css','./app.mjs','./audio.mjs','./chinese-overlay.mjs','./hd-import-ui.mjs','./hd-local-assets.mjs','./hd-background-index.mjs','./hd-pack.mjs','./hd-storage.mjs','./sha256-stream.mjs','./rom-check.mjs','./zh-tw.tsv','./manifest.webmanifest'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(ASSETS)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{
  const {request}=event;
  if(request.method!=='GET'||new URL(request.url).origin!==self.location.origin)return;
  event.respondWith(caches.match(request).then(async cached=>{
    if(cached)return cached;
    const response=await fetch(request);
    if(response.ok && (request.url.endsWith('.wasm')||request.url.includes('/pkg/'))){
      const cache=await caches.open(CACHE);await cache.put(request,response.clone());
    }
    return response;
  }));
});
