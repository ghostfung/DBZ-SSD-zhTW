import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {
  CHEAT_DEFAULTS, CHEAT_FIELDS, CHEAT_STORAGE_KEY, validateCheatChoices,
  readCheatChoices, saveCheatChoices, restoreCheatChoices,
} from '../web/cheat-preferences.mjs';

function store(){
  const map=new Map();
  return {
    getItem:key=>map.has(key)?map.get(key):null,
    setItem:(key,value)=>{map.set(key,String(value));},
    removeItem:key=>{map.delete(key);},
  };
}
function game(){
  const state={...CHEAT_DEFAULTS};
  const calls=[];
  return {
    state,calls,
    cheat_state:()=>JSON.stringify({...state,active:calls.length}),
    set_cheat_option:(key,value)=>{
      calls.push([key,value]);
      if(key==='reset' && value==='all'){
        Object.assign(state,CHEAT_DEFAULTS);
        return;
      }
      if(key==='damage' && value==='5')throw Error('未能寫入作弊選項');
      assert.ok(CHEAT_FIELDS.includes(key),'only whitelisted settings reach the core');
      state[key]=key==='z_cards'?value
        :['max_zeni','no_random'].includes(key)?value==='true':Number(value);
    },
  };
}

test('only the eight first-phase choices are persisted, never ROM or save data',()=>{
  const s=store();
  const input={...CHEAT_DEFAULTS,damage:4,z_cards:'both',no_random:true,
    active:55,session_only:true,rom:'private',save_state:'private'};
  assert.equal(saveCheatChoices(s,input),true);
  const raw=s.getItem(CHEAT_STORAGE_KEY);
  const payload=JSON.parse(raw);
  assert.equal(payload.version,1);
  assert.deepEqual(Object.keys(payload.settings),CHEAT_FIELDS);
  assert.equal(Object.hasOwn(payload.settings,'rom'),false);
  assert.equal(Object.hasOwn(payload.settings,'save_state'),false);
  assert.deepEqual(readCheatChoices(s),{...CHEAT_DEFAULTS,damage:4,z_cards:'both',no_random:true});
});

test('a new game applies previous valid preferences before its first frame',()=>{
  const s=store(),first=game();
  first.state.exp=4;first.state.z_cards='both';first.state.max_zeni=true;
  assert.equal(saveCheatChoices(s,JSON.parse(first.cheat_state())),true);
  const second=game(),status=restoreCheatChoices(second,s);
  assert.deepEqual(status,{found:true,applied:3});
  assert.deepEqual(second.state,first.state);
  assert.deepEqual(second.calls,[['exp','4'],['z_cards','both'],['max_zeni','true']]);
});

test('reset all changes the saved preferences, preventing old cheats next time',()=>{
  const s=store(),first=game();
  first.state.exp=4;first.state.damage=2;
  saveCheatChoices(s,first.state);
  first.set_cheat_option('reset','all');
  assert.equal(saveCheatChoices(s,JSON.parse(first.cheat_state())),true);
  const next=game(),result=restoreCheatChoices(next,s);
  assert.deepEqual(result,{found:true,applied:0});
  assert.deepEqual(next.calls,[]);
  assert.deepEqual(readCheatChoices(s),CHEAT_DEFAULTS);
});

test('importing a save can restore preferences immediately, without embedding them in the save',()=>{
  const s=store(),first=game();
  first.state.exp=3;
  saveCheatChoices(s,first.state);
  // Mirrors the WASM import behavior; no preference write is performed there.
  first.set_cheat_option('reset','all');
  assert.equal(first.state.exp,0);
  assert.equal(readCheatChoices(s).exp,3);
  const afterImport=restoreCheatChoices(first,s);
  assert.equal(afterImport.applied,1);
  assert.equal(first.state.exp,3);
  const next=game(),result=restoreCheatChoices(next,s);
  assert.equal(result.applied,1);
  assert.equal(next.state.exp,3);
});

test('invalid, stale or restricted settings never reach the core',()=>{
  const s=store(),g=game();
  for(const item of [
    '{bad json',
    JSON.stringify({version:2,settings:CHEAT_DEFAULTS}),
    JSON.stringify({version:1,settings:{...CHEAT_DEFAULTS,exp:99}}),
    JSON.stringify({version:1,settings:{...CHEAT_DEFAULTS,damage:-1}}),
    JSON.stringify({version:1,settings:{...CHEAT_DEFAULTS,z_cards:'spawn'}}),
    JSON.stringify({version:1,settings:{...CHEAT_DEFAULTS,no_random:'true'}}),
    JSON.stringify({version:1,settings:{...CHEAT_DEFAULTS,endless:true,gain:15}}),
    JSON.stringify({version:1,settings:{...CHEAT_DEFAULTS,exp:undefined}}),
  ]){
    s.setItem(CHEAT_STORAGE_KEY,item);
    assert.equal(readCheatChoices(s),null);
    assert.deepEqual(restoreCheatChoices(g,s),{found:false,applied:0});
  }
  assert.deepEqual(g.calls,[]);
  assert.equal(validateCheatChoices([]),null);
});

test('Safari storage errors safely use neutral defaults and never prevent play',()=>{
  const broken={getItem(){throw Error('SecurityError');},setItem(){throw Error('QuotaExceededError');}};
  const g=game();
  assert.equal(readCheatChoices(broken),null);
  assert.equal(saveCheatChoices(broken,CHEAT_DEFAULTS),false);
  assert.deepEqual(restoreCheatChoices(g,broken),{found:false,applied:0});
  assert.deepEqual(g.calls,[]);
});

test('partial restore errors revert the active game to neutral without deleting preferences',()=>{
  const s=store(),g=game();
  const settings={...CHEAT_DEFAULTS,exp:4,damage:5};
  assert.equal(saveCheatChoices(s,settings),true);
  const result=restoreCheatChoices(g,s);
  assert.equal(result.found,true);
  assert.equal(result.applied,0);
  assert.match(result.error,/未能寫入/);
  assert.deepEqual(g.state,CHEAT_DEFAULTS);
  assert.deepEqual(readCheatChoices(s),settings);
});

test('PWA wires auto-restore on launch and save on successful change/reset only',()=>{
  const app=readFileSync(new URL('../web/app.mjs',import.meta.url),'utf8');
  const html=readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
  assert.match(app,/import \{saveCheatChoices,restoreCheatChoices\} from '\.\/cheat-preferences\.mjs\?ui=v53'/);
  assert.match(app,/game\.set_cheat_option\(control\.dataset\.cheat,value\);\s*pendingCheatRestore=false;\s*const saved=persistActiveCheats\(\)/);
  assert.match(app,/game\.set_cheat_option\('reset','all'\);\s*pendingCheatRestore=false;\s*const saved=persistActiveCheats\(\)/);
  assert.ok(app.indexOf('const cheatRestore=restoreActiveCheats()')
    >app.indexOf('game = new WebGame(rom, ips)'));
  assert.ok(app.indexOf('const cheatRestore=restoreActiveCheats()')
    <app.indexOf('raf = requestAnimationFrame(requestFrame)'));
  assert.ok(app.indexOf('await applyLoadedSave(new Uint8Array(await record.state.arrayBuffer()))') >= 0); 
  assert.ok(app.indexOf('const importedCheats=restoreActiveCheats()') > app.indexOf('try{game.load_state(bytes);}'));
  assert.match(app,/if\(pendingCheatRestore && n && game\.frame_count\(\)%60===0\)/);
  assert.match(app,/pendingCheatRestore=Boolean\(result\.error\?\.includes\('戰鬥進行中'\)\)/);
  assert.match(html,/設定會自動儲存在本機/);
  assert.match(html,/app\.mjs\?build=v53&ui=v65/);
  assert.match(app,/reconnectFromGesture\(\)/);
});
