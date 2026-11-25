import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
test('v69 gameplay layout is restored; compact import remains',()=>{
 const css=read('web/style.css'),html=read('web/index.html'),app=read('web/app.mjs'),ui=read('web/hd-import-ui.mjs');
 assert.ok(!css.includes('.playing #game.widescreen'));
 assert.ok(!html.includes('id="aspect-toggle"'));
 assert.ok(!app.includes('set_widescreen'));
 assert.ok(app.includes("const BUILD_ID='dbz-hd-tw-pwa-v73'"));
 assert.ok(css.includes('#setup .hd-import-panel .hd-catalog,'));
 assert.ok(css.includes('#setup.managing-files .hd-storage-experimental'));
 assert.ok(html.includes('aria-label="HD 美術包匯入進度"'));
 assert.ok(app.includes("classList.add('managing-files')"));
 const start=ui.indexOf('function progress('),end=ui.indexOf('function refreshAssetList()',start);
 const bar={hidden:true,value:0};let message='';
 const progress=new Function('$','status',ui.slice(start,end)+'\nreturn progress;')(()=>bar,m=>message=m);
 const values=[];
 for(const [done,total,from,to] of [[0,100,0,70],[50,100,0,70],[100,100,0,70],[0,100,70,99],[50,100,70,99],[100,100,70,99]]){
  progress('匯入',done,total,from,to);values.push(bar.value);
  assert.equal(bar.hidden,false);assert.equal(message,'匯入 · '+bar.value+'%');
 }
 assert.deepEqual(values,[0,35,70,70,85,99]);
 assert.ok(ui.includes("status('美術包已匯入並保存 · 100%')"));
 assert.ok(ui.includes("保存失敗，下次需重新選取"));
 assert.ok(ui.includes("HD 素材包未通過驗證"));
 const release=JSON.parse(read('web/offline-release.json'));
 assert.equal(release.version,'v73');
 assert.ok(release.wasmSize>1024);
});
