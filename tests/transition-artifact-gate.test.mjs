import test from 'node:test';
import assert from 'node:assert/strict';
import {TransitionArtifactGate,sampleDarkness} from '../web/transition-artifact-gate.mjs';
const picture=(r,g,b,w=128,h=96)=>{
 const a=new Uint8ClampedArray(w*h*4);
 for(let i=0;i<a.length;i+=4){a[i]=r;a[i+1]=g;a[i+2]=b;a[i+3]=255;}
 return a;
};
test('brief sparse black/sprite transition frame never flashes over HD sky',()=>{
 const w=128,h=96;
 const sky=picture(40,160,220,w,h);
 const sparse=picture(0,0,0,w,h);
 for(let y=40;y<60;y++)for(let x=60;x<84;x++){
    const j=(y*w+x)*4;sparse[j]=60;sparse[j+1]=140;sparse[j+2]=245;
 }
 const gate=new TransitionArtifactGate({maxFrames:5});
 assert.equal(gate.shouldDefer(1,sky,w,h),false);
 gate.didPresent(sky,w,h);
 assert.equal(gate.shouldDefer(2,sparse,w,h),true);
 assert.equal(gate.shouldDefer(3,sparse,w,h),true);
 assert.equal(gate.shouldDefer(4,sky,w,h),false);
 gate.didPresent(sky,w,h);
 assert.equal(gate.heldFramesTotal,2);
});
test('legitimate pure-black fade and stable dark gameplay cannot freeze',()=>{
 const w=128,h=96,sky=picture(50,100,200,w,h);
 const black=picture(0,0,0,w,h);
 const gate=new TransitionArtifactGate();
 gate.didPresent(sky,w,h);
 assert.equal(gate.shouldDefer(15,black,w,h),false);
 gate.didPresent(black,w,h);
 assert.equal(gate.shouldDefer(16,black,w,h),false);
 assert.equal(gate.heldFramesTotal,0);
});
test('suspect frame always fails open in bounded frames and never repeatedly re-arms',()=>{
 const w=128,h=96,sky=picture(70,130,200,w,h),sparse=picture(0,0,0,w,h);
 for(let y=40;y<60;y++)for(let x=60;x<84;x++){
    const j=(y*w+x)*4;sparse[j]=60;sparse[j+1]=140;sparse[j+2]=245;
 }
 const gate=new TransitionArtifactGate({maxFrames:3});
 gate.didPresent(sky,w,h);
 assert.equal(gate.shouldDefer(100,sparse,w,h),true);
 assert.equal(gate.shouldDefer(101,sparse,w,h),true);
 assert.equal(gate.shouldDefer(103,sparse,w,h),false);
 assert.equal(gate.shouldDefer(104,sparse,w,h),false);
 gate.didPresent(sparse,w,h);
 assert.equal(gate.shouldDefer(105,sparse,w,h),false);
});
test('invalid frame dimensions fail open and clear on HD disable',()=>{
 const gate=new TransitionArtifactGate();
 assert.equal(sampleDarkness(new Uint8Array(2),128,96),null);
 assert.equal(gate.shouldDefer(1,new Uint8Array(2),128,96),false);
 assert.equal(gate.shouldDefer(2,picture(10,20,30),128,96,false),false);
 assert.equal(gate.presentedDark,null);
});
