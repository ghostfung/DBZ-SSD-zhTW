import test from 'node:test';
import assert from 'node:assert/strict';
import {assembleTextRuns,reconcileDialogue,drawMenuTranslations} from '../web/screen-text.mjs';

function cells(words,y=175){
  let result=[];
  for(const [start,text] of words)for(const [i,ch] of [...text].entries())
    result.push([start+8*i,y,ch,[11,11,37],[255,238,207]]);
  return result;
}
test('PPU glyph cells group into words without guessing text from an image',()=>{
  const runs=assembleTextRuns(cells([[52,'Enemies'],[52,'approach!']].map(([x,t],i)=>[x+i*100,t])));
  assert.deepEqual(runs.map(x=>x.text),['Enemies','approach!']);
  assert.equal(assembleTextRuns('garbage').length,0);
});
test('exact visible battle notice takes official translation for battle:89, not stale battle:88',()=>{
  const runs=[{text:'Enemies'},{text:'approach!'}];
  const get=key=>key==='battle:89'?'敵人出現了！':'';
  assert.deepEqual(reconcileDialogue({key:'battle:88',text:'…………',stopped:true},runs,get),
    {key:'battle:89',text:'敵人出現了！',progress:1,state:2,stopped:false});
  assert.equal(reconcileDialogue({key:'battle:88',text:'…………'},[],get),null);
  assert.equal(reconcileDialogue({key:'battle:90',text:'正在逃跑'},[],get).text,'正在逃跑');
});
test('menus overlay Chinese within detected English glyph cells and not over dialogue windows',()=>{
  const actions=[];
  const ctx={save(){},restore(){},scale(){},measureText(s){return {width:s.length*10}},
    fillRect(...a){actions.push(['rect',...a])},fillText(...a){actions.push(['text',...a])}};
  const runs=assembleTextRuns(cells([[32,'Atk.'],[128,'Item']]));
  assert.equal(drawMenuTranslations(ctx,512,478,runs,{'Atk.':'戰鬥','Item':'卡牌'}),2);
  assert.ok(actions.some(a=>a[0]==='text'&&a[1]==='戰鬥'));
  assert.equal(drawMenuTranslations(ctx,512,478,runs,{'Atk.':'戰鬥','Item':'卡牌'},
    {left:20,right:205,top:160,bottom:200}),0);
});

test('battle name followed by HP digits translates the name without erasing the digits',()=>{
 const actions=[];
 const ctx={save(){},restore(){},scale(){},measureText(t){return {width:[...t].length*10}},
   fillRect(...a){actions.push(['erase',...a])},fillText(...a){actions.push(['draw',...a])}};
 const runs=assembleTextRuns(cells([[26,'Piccolo77']]));
 assert.equal(drawMenuTranslations(ctx,512,478,runs,{Piccolo:'比克'}),1);
 assert.ok(actions.some(x=>x[0]==='draw'&&x[1]==='比克'));
 assert.equal(actions.find(x=>x[0]==='erase')[3],56,
   'must cover only seven letters, leaving HP 77 visible');
});
