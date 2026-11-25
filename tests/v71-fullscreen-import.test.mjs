import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
test('landscape-only widescreen preference and controls revert on rotation',()=>{
 const app=read('web/app.mjs');
 const start=app.indexOf("const wideLandscape=window.matchMedia"),end=app.indexOf("function updateRenderModeButton()",start);
 const source=app.slice(start,end);
 const classes=new Set(),events=new Map(),attrs=new Map(),button={hidden:false,textContent:'',setAttribute:(k,v)=>attrs.set(k,v),addEventListener:(k,f)=>events.set(k,f)};
 const scene={classList:{contains:k=>classes.has(k),toggle:(k,v)=>v?classes.add(k):classes.delete(k),remove:k=>classes.delete(k)}};
 const landscape={matches:false,addEventListener:(k,f)=>events.set('orientation',f)};
 const calls=[],storage=new Map();
 const env=new Function('$','window','localStorage','calls',
 'let hdEnabled=true,widescreen=true,running=true;const game={set_widescreen:v=>calls.push(v)};const resetControls=()=>{};\n'+source+'\nreturn {syncWidescreen,setHD:v=>{hdEnabled=v;syncWidescreen();}};')(
 id=>id==='aspect-toggle'?button:scene,{matchMedia:()=>landscape,addEventListener(){}},{setItem:(k,v)=>storage.set(k,v)},calls);
 env.syncWidescreen();assert.equal(button.hidden,true);assert.equal(calls.at(-1),false);assert.equal(classes.has('widescreen'),false);
 landscape.matches=true;events.get('orientation')();assert.equal(button.hidden,false);assert.equal(calls.at(-1),true);assert.equal(classes.has('widescreen'),true);
 events.get('click')();assert.equal(button.textContent,'原版比例');assert.equal(classes.has('widescreen'),false);
 events.get('click')();assert.equal(storage.get('dbz-preview-widescreen'),'1');
 landscape.matches=false;events.get('orientation')();assert.equal(classes.has('widescreen'),false);assert.equal(button.hidden,true);
 landscape.matches=true;events.get('orientation')();assert.equal(classes.has('widescreen'),true);
 env.setHD(false);assert.equal(button.hidden,true);assert.equal(classes.has('widescreen'),false);
});
test('overlay styles scoped to landscape widescreen; import diagnostics hidden and progress monotonic',()=>{
 const css=read('web/style.css'),html=read('web/index.html'),ui=read('web/hd-import-ui.mjs');
 assert.ok(css.includes('.playing #game.widescreen .controls button,'));
 assert.ok(css.includes('.playing #game.widescreen .playbar button{opacity:.55}'));
 assert.ok(css.includes('width:min(100vw,calc(100dvh * 16 / 9))'));
 assert.ok(css.includes('#setup .hd-import-panel .hd-catalog,'));
 assert.ok(css.includes('#setup.managing-files .hd-storage-experimental'));
 assert.ok(html.includes('aria-label="HD 美術包匯入進度"'));
 const start=ui.indexOf('function progress('),end=ui.indexOf('function refreshAssetList()',start);
 const bar={hidden:true,value:0};let message='';
 const progress=new Function('$','status',ui.slice(start,end)+'\nreturn progress;')(()=>bar,m=>message=m);
 const values=[];
 for(const [done,total,from,to] of [[0,100,0,70],[50,100,0,70],[100,100,0,70],[0,100,70,99],[50,100,70,99],[100,100,70,99]]){
  progress('匯入',done,total,from,to);values.push(bar.value);assert.equal(bar.hidden,false);assert.equal(message,'匯入 · '+bar.value+'%');
 }
 assert.deepEqual(values,[0,35,70,70,85,99]);
 assert.ok(ui.includes("status('美術包已匯入並保存 · 100%')"));
 assert.ok(ui.includes("保存失敗，下次需重新選取"));
 assert.ok(ui.includes("HD 素材包未通過驗證"));
});
