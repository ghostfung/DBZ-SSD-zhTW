import test from 'node:test';
import assert from 'node:assert/strict';
import {buildHdCoverageAudit,formatHdCoverageAudit}
  from '../web/hd-coverage-audit.mjs';
import {readFileSync} from 'node:fs';
const make=(data,files=[])=>{
  const all=new Set([...Object.keys(data),...files]);
  let reads=0,images=0;
  return {
    images:files.filter(n=>/\.(?:png|webp)$/i.test(n)),
    has:path=>all.has(path),
    readJson:async path=>{reads++;return data[path]},
    readImage:()=>{images++;throw Error('audit must never decode PNG')},
    calls:()=>({reads,images})
  };
};
const face={id:'goku',source:'rom 16:8000 half 0',
  file:'goku.png',palette_colours:{}};
const pose={id:'battle-test',source_key:'1122334455667788',
  source_size:[32,32],file:'battle.png',palette_indexes_used:{
    1:'#ff0000',2:'#00ff00'}};
test('audit detects unsupported pose variants and card tips without image extraction',async()=>{
 const data={
  'bg/index.json':{layers:[]},
  'class1/index.json':{assets:[face]},
  'class2/index.json':{assets_list:[pose,{...pose,id:'luma',file:'luma.png',luma:true}]},
  'cards/index.json':{tiles:[{code:14,base:'card.png'}],
    tips:[{code:14,image:'tip.png'}]},
  'maps/index.json':{version:1,scale:4,tile:256,layers:[
    {id:'field',maps:[1],blocks:[2,1],grid:'06:F300',
     metatiles:'06:F400',tiles:'field/t{x}_{y}.png'}]},
  'mode7/index.json':{version:1,scale:4,tile:256,planes:[]},
  'variants/index.json':{variants:[
    {art:'battle-test',when:'super_saiyan_goku',margin:1,file:'ssj.png'}]}
 };
 const assets=make(data,[
  'class1/goku.png','class2/battle.png','class2/luma.png',
  'cards/card.png','cards/tip.png','variants/ssj.png',
  'maps/field/t0_0.png'
 ]);
 const result=await buildHdCoverageAudit(assets);
 const v=result.categories.find(x=>x.key==='variants');
 assert.equal(v.status,'not_implemented');
 assert.equal(v.rawEntries,1);
 assert.equal(v.existingImages,1);
 const c=result.categories.find(x=>x.key==='cards');
 assert.equal(c.referencedImages,2);
 assert.equal(c.existingImages,2);
 const map=result.categories.find(x=>x.key==='maps');
 assert.equal(map.referencedImages,2);
 assert.equal(map.missingImages,1);
 const missing=result.categories.find(x=>x.key==='field_object');
 assert.equal(missing.status,'index_absent');
 assert.equal(result.unsupported.find(x=>x.kind==='cards/tips').entries,1);
 assert.equal(result.categories.find(x=>x.key==='battle_pose')
  .blockers.find(x=>x.reason.includes('luma')).count,1);
 assert.equal(assets.calls().images,0);
 assert.match(formatHdCoverageAudit(result),/不等於實際遊戲覆蓋率/);
});
test('one malformed index does not hide results for other categories',async()=>{
 const data={
  'cards/index.json':{wrong:[]},
  'class2/index.json':{assets_list:[]},
 };
 const a=make(data);
 const report=await buildHdCoverageAudit(a);
 assert.equal(report.categories.find(c=>c.key==='cards').status,'index_error');
 assert.equal(report.categories.find(c=>c.key==='battle_pose').status,'indexed');
});
test('report is intentionally local and never contains ROM / save data or fabricated success rate',async()=>{
 const data={'class2/index.json':{assets_list:[]}};
 const report=await buildHdCoverageAudit(make(data));
 const text=JSON.stringify(report);
 assert.doesNotMatch(text,/romData|romBytes|saveData|percentage|coveragePercent/);
 assert.equal(report.runtime,null);
});
test('portrait loading success must not clear unrelated HD modules',()=>{
 const code=readFileSync(new URL('../web/hd-import-ui.mjs',import.meta.url),'utf8');
 const start=code.indexOf('async function loadPortraitIndex(source)');
 const end=code.indexOf('function prepareLocalReader',start);
 const portrait=code.slice(start,end);
 for(const cat of ['hd-mode7-catalog-ready','hd-card-catalog-ready',
   'hd-map-catalog-ready','hd-special-catalog-ready'])
   assert.ok(!portrait.includes(cat),'class1 completion must not reset '+cat);
 const onChange=code.slice(code.indexOf("$('hd-zip').addEventListener('change'"),
   code.indexOf("$('hd-verify').addEventListener('click'"));
 assert.match(onChange,/for\(const kind of \['mode7','card','map','special','portrait','fighter'\]/);
 assert.match(onChange,/'hd-'\+kind\+'-catalog-ready'/);
});
test('audit must remain collapsed away from the mobile controller',()=>{
 const html=readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
 assert.ok(html.indexOf('class="controls"')<
   html.indexOf('id="hd-coverage-build"'));
 assert.match(html,/<details id="game-diagnostics" class="game-diagnostics">/);
 assert.match(html,/id="hd-coverage-export"[^>]*disabled/);
});

test('luma is explicitly supported in current audit, without inventing game hits',async()=>{
 const item={id:'battle-test',source_key:'1234',source_size:[2,2],
   file:'grey.png',luma:true,palette_indexes_used:{}};
 const report=await buildHdCoverageAudit(make({
   'class2/index.json':{assets_list:[item]}
 },['class2/grey.png']));
 const result=report.categories.find(x=>x.key==='battle_pose');
 assert.ok(!result.blockers.some(x=>x.reason.includes('尚未支援')));
 assert.equal(result.blockers.find(x=>x.reason.includes('luma 符合')).count,1);
 report.runtime={status:'sampled',categories:{
   battle_pose:{lumaMatches:1,lumaReady:1},
   field_object_and_effect:{lumaMatches:null,lumaReady:null}
 }};
 assert.match(formatHdCoverageAudit(report),/戰鬥人物 luma 實際匹配：1 組/);
});
