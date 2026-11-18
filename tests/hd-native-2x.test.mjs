import test from 'node:test';
import assert from 'node:assert/strict';
import {decodeBackground,LiveHdArt} from '../web/hd-live.mjs';

test('official 4x HD art is downsampled to 2x once, before crossing the WASM boundary',async()=>{
  const originalBitmap=globalThis.createImageBitmap;
  const originalDocument=globalThis.document;
  const calls=[];
  let closed=0;
  globalThis.createImageBitmap=async()=>({width:1024,height:64,close(){closed++}});
  globalThis.document={createElement(type){
    assert.equal(type,'canvas');
    const canvas={width:0,height:0};
    canvas.getContext=()=>{
      const ctx={imageSmoothingEnabled:false,imageSmoothingQuality:'low',
        drawImage(...a){calls.push({a,smooth:this.imageSmoothingEnabled,
          quality:this.imageSmoothingQuality,width:canvas.width,height:canvas.height});},
        getImageData(x,y,w,h){return {data:new Uint8Array(w*h*4)};}};
      return ctx;
    };
    return canvas;
  }};
  try {
    const art=await decodeBackground({readImage:async()=>new Uint8Array([1,2,3])},'bg/test.webp');
    assert.equal(art.width,512);
    assert.equal(art.height,32);
    assert.equal(art.rgba.byteLength,512*32*4);
    assert.deepEqual([calls[0].smooth,calls[0].quality], [true,'high']);
    assert.deepEqual(calls[0].a.slice(1),[0,0,512,32]);
    assert.equal(closed,1);
  } finally {
    globalThis.createImageBitmap=originalBitmap;
    globalThis.document=originalDocument;
  }
});

test('reject odd-size art and always free the browser image bitmap',async()=>{
  const prev=globalThis.createImageBitmap;
  let closed=0;
  globalThis.createImageBitmap=async()=>({width:513,height:32,close(){closed++}});
  try {
    await assert.rejects(()=>decodeBackground({readImage:async()=>new Uint8Array([1])},'bg/odd.png'),/偶數/);
    assert.equal(closed,1);
  } finally {globalThis.createImageBitmap=prev;}
});

test('late Safari decode does not install outdated art after scene changed',async()=>{
  let finish;
  const pending=new Promise(resolve=>{finish=resolve;});
  let matches=[{index:0,id:'a',art:'bg/a.webp',ready:false}];
  const calls=[];
  const game={
    set_background_index(){}, hd_mode(){return true;},
    background_matches(){return JSON.stringify(matches);},
    hd_art_pixels(){return 0;},
    set_background_art(...a){calls.push(a);}
  };
  const live=new LiveHdArt(()=>{},()=>pending);
  live.setCatalog({items:[{id:'a',art:'bg/a.webp'}]},{readImage(){}});
  live.attach(game);
  live.pulse();
  matches=[]; live.pulse();
  finish({width:2,height:2,rgba:new Uint8Array(16)});
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(calls.length,0);
  assert.equal(live.failed.size,0);
});

test('a transient Safari decoder failure retries a bounded time within the same scene',async()=>{
  let failures=0, ready=false;
  const game={
    set_background_index(){},hd_mode(){return true;},
    background_matches(){return JSON.stringify([{index:0,id:'a',art:'bg/a.webp',ready}]);},
    set_background_art(){ready=true;}
  };
  const live=new LiveHdArt(()=>{},async()=>{
    if(!failures++)throw Error('transient decode failure');
    return {width:2,height:2,rgba:new Uint8Array(16)};
  });
  live.setCatalog({items:[{id:'a',art:'bg/a.webp'}]},{readImage(){}});
  live.attach(game);
  live.pulse();
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(live.failed.get(0)?.retries,1);
  live.pulse();live.pulse();
  assert.equal(failures,1);
  live.pulse();
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(ready,true);
  assert.equal(live.failed.size,0);
});
