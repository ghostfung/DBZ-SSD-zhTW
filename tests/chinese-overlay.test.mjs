import test from 'node:test';
import assert from 'node:assert/strict';
import {findDialogueBox,drawChineseInBox} from '../web/chinese-overlay.mjs';

const W=256,H=239;
function mockFrame(scale=1) {
  const w=W*scale,h=H*scale;
  const rgba=new Uint8Array(w*h*4);
  for(let y=0;y<h;y++)for(let x=0;x<w;x++){
    let i=(y*w+x)*4;
    rgba[i]=189;rgba[i+1]=222;rgba[i+2]=239;rgba[i+3]=255;
    const lx=x/scale,ly=y/scale;
    if(lx>=22 && lx<=232 && ly>=172 && ly<=226) {
      rgba[i]=255;rgba[i+1]=239;rgba[i+2]=206;
    }
  }
  return rgba;
}

test('locate classic ivory text window without consuming game data',()=>{
  const frame=mockFrame();
  const result=findDialogueBox(frame,W,H);
  assert.ok(result);
  assert.equal(result.top,172);
  assert.equal(result.bottom,226);
  assert.equal(result.fill,'rgb(255,239,206)');
  assert.equal(findDialogueBox(new Uint8Array(W*H*4),W,H),null);
});
test('locate corresponding dialogue window in actual 2x Enhanced dimensions',()=>{
  const result=findDialogueBox(mockFrame(2),512,478);
  assert.ok(result);
  assert.equal(result.sx,2);
  assert.equal(result.sy,2);
});
test('draws CJK text in detected window and falls back when missing',()=>{
  const actions=[];
  const ctx={
    save(){actions.push('save')},restore(){actions.push('restore')},
    scale(x,y){actions.push(['scale',x,y])},
    measureText(v){return {width:[...v].length*12}},
    fillRect(...a){actions.push(['fillRect',...a])},
    fillText(...a){actions.push(['fillText',...a])},
    beginPath(){},moveTo(){},lineTo(){},fill(){}
  };
  const message={text:'突然來訪的賽亞人拉帝茲，\\n自稱是悟空的哥哥，'.replace('\\n','\n'),progress:1,state:2};
  assert.equal(drawChineseInBox(ctx,mockFrame(),W,H,message),true);
  assert.ok(actions.some(x=>Array.isArray(x)&&x[0]==='fillRect'));
  assert.ok(actions.some(x=>Array.isArray(x)&&x[0]==='fillText'&&x[1].includes('拉帝茲')));
  assert.equal(drawChineseInBox(ctx,new Uint8Array(W*H*4),W,H,message),false);
});


function mockBattleFrame(scale=1){
  const w=W*scale,h=H*scale;
  const rgba=new Uint8Array(w*h*4);
  for(let y=0;y<h;y++)for(let x=0;x<w;x++){
    const i=(y*w+x)*4,lx=x/scale,ly=y/scale;
    rgba[i]=25;rgba[i+1]=103;rgba[i+2]=198;rgba[i+3]=255;
    if(lx>=48 && lx<=207 && ly>=177 && ly<=238){
      rgba[i]=255;rgba[i+1]=239;rgba[i+2]=206;
    }
  }
  return rgba;
}
test('locate right-shifted battle message box, not only story windows at x=26',()=>{
  for(const scale of [1,2]){
    const frame=mockBattleFrame(scale);
    const box=findDialogueBox(frame,W*scale,H*scale);
    assert.ok(box,'the narrower battle window must be detected');
    assert.ok(box.left>=44 && box.left<=52);
    assert.ok(box.right>=203 && box.right<=209);
    assert.equal(box.top,177);
    assert.ok(box.bottom>=236);
  }
});
