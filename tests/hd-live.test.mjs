import test from 'node:test';
import assert from 'node:assert/strict';
import {LiveHdArt} from '../web/hd-live.mjs';

test('HD catalogue is not treated as rendered without a PPU match', () => {
  let text='';
  const live=new LiveHdArt(value=>{text=value;});
  live.setCatalog({items:[{id:'test',art:'bg/test.png'}]}, {readImage(){}});
  assert.match(text,/等待 PPU/);
  live.attach({set_background_index(){}, hd_mode(){return true;},
    background_matches(){return '[]';}});
  live.pulse();
  assert.match(text,/未符合/);
});

test('matched HD art is sent to WASM locally and only once', async () => {
  const calls=[];
  let ready=false;
  const game={
    set_background_index(){},
    hd_mode(){return true;},
    background_matches(){return JSON.stringify([{index:0,id:'test',art:'bg/test.webp',ready}]);},
    set_background_art(index,width,height,rgba){calls.push([index,width,height,rgba.length]);ready=true;}
  };
  const decoder=async()=>({width:4,height:4,rgba:new Uint8Array(64)});
  const live=new LiveHdArt(()=>{},decoder);
  live.setCatalog({items:[{id:'test',art:'bg/test.webp'}]}, {readImage(){}});
  live.attach(game);
  live.pulse();
  await new Promise(resolve=>setImmediate(resolve));
  live.pulse();
  assert.deepEqual(calls,[[0,4,4,64]]);
});

test('switching games drops an in-flight HD image instead of applying it to the new game', async () => {
  let finish;
  const pending=new Promise(resolve=>{finish=resolve;});
  const game={set_background_index(){},hd_mode(){return true;},
    background_matches(){return '[{"index":0,"id":"test","art":"bg/test.webp","ready":false}]';},
    set_background_art(){throw Error('stale result installed');}};
  const live=new LiveHdArt(()=>{},()=>pending);
  live.setCatalog({items:[{id:'test',art:'bg/test.webp'}]}, {readImage(){}});
  live.attach(game);
  live.pulse();
  live.detach();
  finish({width:4,height:4,rgba:new Uint8Array(64)});
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(live.game,null);
});
