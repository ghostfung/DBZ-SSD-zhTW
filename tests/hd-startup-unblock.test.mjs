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

test('WASM uses offline CacheStorage bytes, or native direct URL when not cached',()=>{
 const app=source('web/app.mjs'),sw=source('web/service-worker.js');
 assert.match(app,/ssd_web\\.js\\?build=v53/);
 assert.match(app,/ssd_web_bg\\.wasm\\?build=v53/);
 assert.match(app,/const offlineWasm=await loadOfflineWasm\\(wasmUrl\\)/);
 assert.match(app,/module_or_path:offlineWasm\\?\\?wasmUrl/);
 assert.doesNotMatch(app,/module_or_path:wasmResponse/);
 assert.doesNotMatch(app,/new AbortController\\(\\)/);
 assert.match(sw,/uri\\.pathname\\.endsWith\\('\/pkg\/ssd_web_bg\\.wasm'\\)/);
 assert.doesNotMatch(sw,/Response\\.clone\\(/);
});
test('old SW unregister is removed and v59 installs offline shell',()=>{
 const html=source('web/index.html'),offline=source('web/offline-installer.mjs');
 assert.doesNotMatch(html,/registration\\.unregister\\(\\)/);
 assert.match(offline,/serviceWorker\\.getRegistration/);
 assert.match(offline,/serviceWorker\\.register/);
 assert.match(html,/offline-installer\\.mjs\\?offline=v59/);
 assert.match(html,/app\\.mjs\\?build=v53/);
});

test('v42 display optimization survives and V44 core build matches client',()=>{
 const app=source('web/app.mjs'),rust=source('crates/ssd-web/src/lib.rs');
 assert.match(app,/const transientOnly=holdReasons\.length>0&&!storyHold&&!hdHold/);
 assert.match(app,/event\.detail\.framePerformance=/);
 assert.match(rust,/dbz-hd-tw-pwa-v53/);
});
