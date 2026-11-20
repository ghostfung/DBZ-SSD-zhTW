import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('startup shows a distinct stage and restores button on WASM failure',()=>{
 const app=source('web/app.mjs');
 for(const stage of ['正在讀取 ROM／IPS','正在載入遊戲程式模組',
   '正在直接下載及編譯 WASM','正在建立遊戲核心',
   '正在載入繁體中文翻譯','正在套用 HD 美術索引'])
   assert.ok(app.includes(stage),stage);
 assert.match(app,/setStatus\('遊戲啟動失敗（'/);
 assert.match(app,/button\.disabled=false/);
 assert.match(app,/button\.textContent=originalLabel/);
});

test('WASM uses proven wasm-pack URL loader, not WebKit intercepted Response',()=>{
 const app=source('web/app.mjs'),sw=source('web/service-worker.js');
 assert.match(app,/ssd_web\.js\?build=v44/);
 assert.match(app,/ssd_web_bg\.wasm\?build=v44/);
 assert.match(app,/mod\.default\(\{module_or_path:wasmUrl\}\)/);
 assert.doesNotMatch(app,/module_or_path:wasmResponse/);
 assert.doesNotMatch(app,/new AbortController\(\)/);
 assert.doesNotMatch(app,/serviceWorker\.register/);
 assert.doesNotMatch(sw,/addEventListener\('fetch'/);
 assert.doesNotMatch(sw,/respondWith|caches\.put|Response\.clone/);
});

test('legacy service workers are retired before file selection',()=>{
 const html=source('web/index.html');
 assert.match(html,/navigator\.serviceWorker\.getRegistration\(\)/);
 assert.match(html,/registration\.unregister\(\)/);
 assert.match(html,/if \(removed && wasControlled\) location\.reload\(\)/);
 assert.ok(html.indexOf('registration.unregister')<html.indexOf('id="hd-zip"'));
 assert.match(html,/app\.mjs\?build=v44/);
});

test('v42 display optimization survives and V44 core build matches client',()=>{
 const app=source('web/app.mjs'),rust=source('crates/ssd-web/src/lib.rs');
 assert.match(app,/const transientOnly=holdReasons\.length>0&&!storyHold&&!hdHold/);
 assert.match(app,/event\.detail\.framePerformance=/);
 assert.match(rust,/hd-safari-network-v44/);
});
