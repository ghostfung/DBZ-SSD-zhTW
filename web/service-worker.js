// v44: retire the legacy offline worker. WASM must stream directly from
// the deployment, not via FetchEvent.respondWith, caches.put or Response.clone.
// This file intentionally has no fetch listener and is NOT registered by app.mjs.
self.addEventListener('install',event=>event.waitUntil(self.skipWaiting()));
self.addEventListener('activate',event=>event.waitUntil(self.registration.unregister()));
