import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('startup shows a distinct stage and restores button on WASM failure',()=>{
 const app=source('web/app.mjs');
 for(const stage of ['正在讀取 ROM／IPS','正在載入遊戲程式模組',
   '正在直接下載及編譯 WASM','正在建立遊戲核心',
   '正在載入','正在套用 HD 美術索引'])
   assert.ok(app.includes(stage),stage);
 assert.match(app,/setStatus\('遊戲啟動失敗（'/);
 assert.match(app,/button\.disabled=false/);
 assert.match(app,/button\.textContent=originalLabel/);
});

test('WASM uses cached bytes or direct network URL, not a WebKit SW stream',()=>{
 const app=source('web/app.mjs'),sw=source('web/service-worker.js');
 assert.ok(app.includes('ssd_web.js?build=v53'));
 assert.ok(app.includes('ssd_web_bg.wasm?build=v53'));
 assert.ok(app.includes('const offlineWasm=await loadOfflineWasm(wasmUrl)'));
 assert.ok(app.includes('module_or_path:offlineWasm??wasmUrl'));
 assert.ok(!app.includes('module_or_path:wasmResponse'));
 assert.ok(!app.includes('new AbortController()'));
 assert.ok(sw.includes("uri.pathname.endsWith('/pkg/ssd_web_bg.wasm')"));
 assert.ok(!sw.includes('Response.clone('));
});
test('legacy unregister is gone, current worker installs offline shell',()=>{
 const html=source('web/index.html'),offline=source('web/offline-installer.mjs');
 assert.ok(!html.includes('registration.unregister('));
 assert.ok(offline.includes('serviceWorker.getRegistration'));
 assert.ok(offline.includes('serviceWorker.register'));
 assert.ok(html.includes('offline-installer.mjs?offline=v60'));
 assert.ok(html.includes('app.mjs?build=v53'));
});
test('v42 display optimization survives and V44 core build matches client',()=>{
 const app=source('web/app.mjs'),rust=source('crates/ssd-web/src/lib.rs');
 assert.match(app,/const transientOnly=holdReasons\.length>0&&!storyHold&&!hdHold/);
 assert.match(app,/event\.detail\.framePerformance=/);
 assert.match(rust,/dbz-hd-tw-pwa-v53/);
});
