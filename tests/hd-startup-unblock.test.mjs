import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');

test('startup shows stages instead of leaving a grey inert button',()=>{
 const app=source('web/app.mjs');
 for(const stage of ['正在讀取 ROM／IPS','正在載入遊戲程式模組',
    '正在下載 WebAssembly 遊戲核心','正在建立遊戲核心',
    '正在載入繁體中文翻譯','正在套用 HD 美術索引'])
   assert.ok(app.includes(stage),stage);
 assert.match(app,/setStatus\('遊戲啟動失敗（'/);
 assert.match(app,/button\.disabled=false/);
 assert.match(app,/button\.textContent=originalLabel/);
});

test('WASM has release-keyed URL, explicit MIME check and no hanging SW cache write',()=>{
 const app=source('web/app.mjs'),sw=source('web/service-worker.js');
 assert.match(app,/ssd_web\.js\?build=v43/);
 assert.match(app,/ssd_web_bg\.wasm\?build=v43/);
 assert.match(app,/mod\.default\(\{module_or_path:wasmResponse\}\)/);
 assert.match(app,/new AbortController\(\)/);
 assert.match(app,/application\\\/wasm/);
 assert.match(sw,/const isWasm=path\.endsWith\('\.wasm'\)/);
 assert.match(sw,/cache:isWasm\?'default':'no-store'/);
 assert.match(sw,/if\(response\.ok&&cacheable&&!isWasm\)/);
 assert.doesNotMatch(sw,/await cache\.put/);
 assert.doesNotMatch(sw,/ignoreSearch:true/);
});

test('startup keeps the v42 Luma/performance changes and matching WASM build tag',()=>{
 const app=source('web/app.mjs'),rust=source('crates/ssd-web/src/lib.rs');
 assert.match(app,/const transientOnly=holdReasons\.length>0&&!storyHold&&!hdHold/);
 assert.match(app,/event\.detail\.framePerformance=/);
 assert.match(rust,/hd-startup-unblock-v43/);
});
