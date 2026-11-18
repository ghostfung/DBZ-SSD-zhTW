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


function mockPortraitFrame(scale=1) {
  const width=W*scale,height=H*scale;
  const rgba=new Uint8Array(width*height*4);
  for(let y=0;y<height;y++)for(let x=0;x<width;x++){
    const lx=x/scale,ly=y/scale;
    const i=(y*width+x)*4;
    let colour=[24,180,120];
    if(lx>=18 && lx<=239 && ly>=151 && ly<=238) colour=[255,239,206];
    // The portrait interrupts the upper-left part of the ivory background.
    if(lx>=30 && lx<70 && ly>=157 && ly<=195) colour=[37,33,27];
    for(const ty of [160,176,192]) {
      if(lx>=75 && lx<=225 && ly>=ty && ly<ty+6 &&
         Math.floor((lx-75)/8)%2===0) colour=[9,9,35];
    }
    rgba.set([...colour,255],i);
  }
  return rgba;
}
test('portrait dialogue covers the FIRST English line and preserves the avatar',()=>{
  const actions=[];
  const ctx={save(){},restore(){},scale(){},
    measureText(v){return {width:[...v].length*11}},
    fillRect(...a){actions.push(['rect',...a])},
    fillText(...a){actions.push(['text',...a])},
    beginPath(){},moveTo(){},lineTo(){},fill(){}};
  for(const scale of [1,2]){
    const frame=mockPortraitFrame(scale);
    const box=findDialogueBox(frame,W*scale,H*scale);
    assert.ok(box);
    assert.ok(box.top<=155,'expected the upper ivory text band, got '+box.top);
    assert.ok(box.bottom>=232);
    actions.length=0;
    const dialogue={text:'悟空，可別死啊！',portrait:true,progress:1,state:2};
    const runs=[{x:75,y:160,text:"I'd offer"},{x:78,y:176,text:'but I am'}];
    assert.equal(drawChineseInBox(ctx,frame,W*scale,H*scale,dialogue,box,runs),true);
    const cover=actions.find(a=>a[0]==='rect');
    const line=actions.find(a=>a[0]==='text');
    assert.ok(cover[1]>=68,'portrait was covered: '+cover);
    assert.ok(cover[2]<=156,'upper English was not cleared: '+cover);
    assert.ok(line[3]<=165,'Chinese was positioned on a later row: '+line);
  }
});

test('unresolved English is masked only inside the current dialogue, not over sprites', async()=>{
  const actions=[];
  const ctx={save(){},restore(){},scale(){},fillRect(...args){actions.push(args)}};
  const {maskPendingDialogue}=await import('../web/chinese-overlay.mjs');
  const box={left:18,right:239,top:151,bottom:237,fill:'rgb(255,239,206)',sx:2,sy:2};
  const runs=[{x:77,y:160,text:'I'},{x:85,y:160,text:'would'},{x:80,y:176,text:'help'}];
  assert.equal(maskPendingDialogue(ctx,512,478,box,runs,{}),true);
  assert.ok(actions[0][0]>=70,'the avatar remains untouched');
  assert.ok(actions[0][1]<=154,'top English row is cleared');
  assert.equal(maskPendingDialogue(ctx,512,478,{...box,top:20,bottom:85},runs,{}),false);
  assert.equal(maskPendingDialogue(ctx,512,478,box,[{x:50,y:161,text:'Menu'}],{Menu:'選單'}),false);
});
