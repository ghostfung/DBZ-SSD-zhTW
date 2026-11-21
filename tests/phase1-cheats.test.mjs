import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const src=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const bridge=src('crates/ssd-web/src/cheat_bridge.rs');
const wasm=src('crates/ssd-web/src/lib.rs');
const app=src('web/app.mjs');
const html=src('web/index.html');
const css=src('web/style.css');

test('only the first-phase upstream CPU hooks are enabled in the PWA',()=>{
  const hookList=bridge.split('const ALLOWED_HOOKS:')[1].split('];')[0];
  for(const hook of ['cheat_damage','cheat_exp','cheat_group',
    'cheat_deal','cheat_roll','cheat_money','cheat_sale']){
    assert.match(hookList,new RegExp('"'+hook+'"'));
  }
  for(const blocked of ['cheat_item','cheat_power','cheat_fighter_',
    'story_','cheat_pick']){
    assert.doesNotMatch(hookList,new RegExp(blocked));
  }
  assert.match(bridge,/if ALLOWED_HOOKS\.contains\(&entry\.name\.as_str\(\)\)/);
  assert.match(bridge,/machine\.set_hook_table\(None\)/);
  assert.match(bridge,/engine::frame_events\(machine\.wram\(\)\)/);
  assert.match(bridge,/BATTLE_MODES\.contains\(&mode\)/);
});

test('no item spawn, infinite items, enemies, save editor or data packs in cheats UI',()=>{
  const panel=html.slice(html.indexOf('id="cheats-panel"'));
  for(const key of ['damage','defence','exp','z_cards','gain',
    'encounters','max_zeni','no_random']){
    assert.match(panel,new RegExp('data-cheat="'+key+'"'));
  }
  for(const excluded of ['data-cheat="endless"','data-cheat="item_spawner"',
    'data-cheat="enemy_power"','data-cheat="unlock_chapters"','id="save-editor"']){
    assert.doesNotMatch(panel,new RegExp(excluded));
  }
  assert.match(bridge,/"item_spawner"/);
  assert.match(bridge,/"unlock_chapters"/);
  assert.match(bridge,/"enemy_power"/);
  assert.match(html,/id="cheat-open"/);
  assert.match(html,/id="cheat-feedback"/);
});

test('WASM cheats are session-only and state restore resets stale WRAM cheat blocks',()=>{
  assert.match(wasm,/cheats: Cheats::neutral\(\)/);
  assert.match(wasm,/pub fn cheat_state\(&self\) -> String/);
  assert.match(wasm,/pub fn set_cheat_option\(&mut self, key: &str, value: &str\)/);
  assert.match(wasm,/cheat_bridge::before_frame\(&mut self\.machine, self\.cheats\)/);
  assert.match(wasm,/self\.cheats = Cheats::neutral\(\)/);
  assert.match(wasm,/cheat_bridge::clear_after_state_load\(&mut self\.machine\)/);
  assert.match(bridge,/engine::block_events\(&Cheats::neutral\(\)\)/);
  assert.match(bridge,/"session_only": true/);
  const rawSaveBody=wasm.split('pub fn save_state(&self) -> Vec<u8>')[1].split('pub fn load_state')[0];
  assert.doesNotMatch(rawSaveBody,/cheat_bridge/);
});

test('mobile menu pauses gameplay, preserves sound recovery and resumes after closing',()=>{
  assert.match(app,/let cheatPanelOpen = false/);
  assert.match(app,/function openCheatPanel\(\)/);
  assert.match(app,/if\(running\)\{running=false;cancelAnimationFrame\(raf\);\}/);
  assert.match(app,/function closeCheatPanel\(\)/);
  assert.match(app,/if\(!running&&!cheatPanelOpen\)/);
  assert.match(app,/await sound\.reconnectFromGesture\(\)/);
  assert.match(app,/refreshCheatPanel\('讀取進度成功；作弊設定已自動關閉'\)/);
  assert.match(css,/\.cheats-modal\{/);
  assert.match(css,/overscroll-behavior:contain/);
  assert.match(html,/app\.mjs\?build=v50&ui=v53/);
  assert.match(wasm,/dbz-hd-tw-pwa-v50/);
});
