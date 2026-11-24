// Cache-first, version-neutral offline runtime. Cache pointers move only after
// complete, validated updates and a player-approved restart. Never read ROMs,
// save states, IPS or HD ZIP. Direct WASM loading avoids iOS streaming hangs.
const CONTROL='dbz-ssd-offline-control-v1';
const DEFAULT='dbz-ssd-offline-v62';
self.addEventListener('install',event=>event.waitUntil(self.skipWaiting()));
self.addEventListener('activate',event=>event.waitUntil(self.clients.claim()));
async function activeCache(){
  try{
    const control=await caches.open(CONTROL);
    const response=await control.match(
      new URL('./__dbz-offline-active__',self.registration.scope).href);
    if(!response)return DEFAULT;
    const entry=await response.json();
    if(typeof entry.version==='string'&&/^v[1-9][0-9]*(?:[.][0-9]+){0,2}$/.test(entry.version))
      return 'dbz-ssd-offline-'+entry.version;
  }catch{}
  return DEFAULT;
}
self.addEventListener('fetch',event=>{
  const req=event.request;
  if(req.method!=='GET')return;
  const url=new URL(req.url);
  if(url.origin!==self.location.origin)return;
  // WASM never travels through respondWith() (WebKit previously failed).
  // Metadata must bypass every offline cache to detect new releases.
  if(url.pathname.endsWith('/pkg/ssd_web_bg.wasm')||
     url.pathname.endsWith('/service-worker.js')||
     url.pathname.endsWith('/offline-release.json'))return;
  // Installer uses reload to request fresh assets, not old cache entries.
  // Navigations retain offline fallback even on user-initiated reload.
  if((req.cache==='reload'||req.cache==='no-store')&&req.mode!=='navigate')return;
  event.respondWith((async()=>{
    const cache=await caches.open(await activeCache());
    const result=await cache.match(req,{ignoreSearch:true});
    if(result)return result;
    try{return await fetch(req);}
    catch(error){
      if(req.mode==='navigate'){
        const fallback=await cache.match(
          new URL('./index.html',self.registration.scope).href,{ignoreSearch:true});
        if(fallback)return fallback;
      }
      throw error;
    }
  })());
});
