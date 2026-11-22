// v59: cache-first static app shell. NEVER cache users' ROM or HD art.
const CACHE='dbz-ssd-offline-v59';
self.addEventListener('install',e=>e.waitUntil(self.skipWaiting()));
self.addEventListener('activate',e=>e.waitUntil(self.clients.claim()));
self.addEventListener('fetch',e=>{
  const request=e.request;
  if(request.method!=='GET')return;
  const uri=new URL(request.url);
  if(uri.origin!==self.location.origin)return;
  // WASM is fetched directly (no SW stream interception). Offline engine
  // passes cached Uint8Array straight to WebAssembly.instantiate().
  if(uri.pathname.endsWith('/pkg/ssd_web_bg.wasm')||uri.pathname.endsWith('/service-worker.js'))return;
  if((request.cache==='reload'||request.cache==='no-store')&&request.mode!=='navigate')return;
  e.respondWith((async()=>{
    const c=await caches.open(CACHE);
    const hit=await c.match(request,{ignoreSearch:true});
    if(hit)return hit;
    try{return await fetch(request);}
    catch(err){
      if(request.mode==='navigate'){
        const fallback=await c.match(new URL('./index.html',self.registration.scope).href,{ignoreSearch:true});
        if(fallback)return fallback;
      }
      throw err;
    }
  })());
});
