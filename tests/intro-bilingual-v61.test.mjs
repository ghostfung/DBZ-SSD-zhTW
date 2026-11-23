import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,statSync} from 'node:fs';
const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
test('original intro English is masked before HD upscale and its title font is omitted',()=>{
 const enhanced=read('crates/ssd-hd/src/enhanced.rs');
 const translator=read('crates/ssd-hd/src/translate.rs');
 assert.match(enhanced,/translator\.clean_intro\(/);
 assert.match(enhanced,/translator\.quiet_layers\(\)/);
 assert.match(enhanced,/translator\.font_cells\(all_cells\)/);
 assert.ok(enhanced.indexOf('translator.clean_intro(')<
   enhanced.indexOf('font.draw_with('),'intro should be cleaned before enlarged English is re-drawn');
 assert.ok(translator.includes('top + INTRO_EM as i32'));
});
test('intro translations are complete in TW and HK; other scenes retain source text',()=>{
 for(const lang of ['zh-tw','zh-hk']){
   const entries=read('web/'+lang+'.tsv').split(/\r?\n/);
   for(let i=0;i<32;i++){
     assert.ok(entries.some(line=>line.startsWith('intro:'+i+'\t')&&
       line.split('\t')[1]?.trim()),lang+' missing intro:'+i);
   }
 }
 const translator=read('crates/ssd-hd/src/translate.rs');
 assert.ok(translator.includes('filter(|screen| screen.table == INTRO_TABLE)'));
});
test('v61 retains offline update and keeps ROM/HD/save stores untouched',()=>{
 const html=read('web/index.html'),app=read('web/app.mjs');
 assert.ok(html.includes('offline-installer.mjs?offline=v61'));
 assert.ok(app.includes("offline-installer.mjs?offline=v61"));
 assert.ok(html.includes('id="save-slots-panel"'));
 assert.ok(html.includes('id="hd-toggle"'));
 assert.equal(JSON.parse(read('web/offline-release.json')).version,'v61');
 assert.equal(JSON.parse(read('web/offline-release.json')).wasmSize,statSync(new URL('../web/pkg/ssd_web_bg.wasm',import.meta.url)).size);
});
