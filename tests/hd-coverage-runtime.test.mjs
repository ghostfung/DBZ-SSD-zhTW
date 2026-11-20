import test from 'node:test';
import assert from 'node:assert/strict';
import {attachHdRuntimeSnapshot} from '../web/hd-coverage-runtime.mjs';
test('disabled HD is marked not_sampled and is never misreported as seven zero matches',()=>{
 const report={runtime:null};
 const game={frame_count:()=>559,hd_active:()=>false,
   fighter_matches:()=>{throw Error('must not query disabled runtime');}};
 attachHdRuntimeSnapshot(report,game);
 assert.equal(report.runtime.status,'not_sampled');
 assert.equal(report.runtime.hdActive,false);
 assert.equal(report.runtime.frame,559);
 assert.equal(report.runtime.categories,null);
 assert.match(report.runtime.reason,/HD 2x/);
});
test('live frame keeps unsupported match methods distinct from actual zero matches',()=>{
 const report={};
 const game={frame_count:()=>1250,hd_active:()=>true,
   fighter_matches:()=>JSON.stringify([{ready:true},{ready:false}]),
   hd_fighter_pixels:()=>128,
   portrait_matches:()=>'[]',
   hd_portrait_pixels:()=>0};
 attachHdRuntimeSnapshot(report,game);
 assert.equal(report.runtime.status,'sampled');
 assert.deepEqual(report.runtime.categories.battle_pose,
   {matches:2,ready:1,lumaMatches:null,lumaReady:null,
    replacedPixels:128,matchDataAvailable:true});
 assert.equal(report.runtime.categories.portrait.matches,0);
 assert.equal(report.runtime.categories.background.matches,null);
 assert.equal(report.runtime.categories.background.matchDataAvailable,false);
});
test('no game does not invent a valid frame',()=>{
 const r={};attachHdRuntimeSnapshot(r,null);
 assert.equal(r.runtime.status,'not_sampled');
 assert.equal(r.runtime.frame,null);
});

test('luma usage is reported only when exact runtime match includes a luma flag',()=>{
 const report={};
 attachHdRuntimeSnapshot(report,{hd_active:()=>true,frame_count:()=>211,
  fighter_matches:()=>JSON.stringify([
    {index:5,luma:true,ready:true},
    {index:6,luma:false,ready:true},
    {index:8,luma:true,ready:false}]),
  hd_fighter_pixels:()=>87
 });
 const pose=report.runtime.categories.battle_pose;
 assert.equal(pose.matches,3);
 assert.equal(pose.lumaMatches,2);
 assert.equal(pose.lumaReady,1);
 assert.equal(pose.replacedPixels,87);
});
