import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JoypadInput} from '../web/joypad-input.mjs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');
test('ABXY first hold then repeat with real release edges',()=>{
 for(const bit of [0,1,8,9]){
  const input=new JoypadInput();input.press(bit,'finger',true);
  const samples=Array.from({length:40},()=>input.sample());
  assert.deepEqual(samples, [...Array(24).fill(1<<bit),...Array(4).fill(0),...Array(4).fill(1<<bit),...Array(4).fill(0),...Array(4).fill(1<<bit)]);
  input.release(bit,'finger');assert.equal(input.sample(),0);
 }
});
test('direction and keyboard stay held; multitouch and short taps remain independent',()=>{
 const i=new JoypadInput();i.press(4,'up');i.press(8,'a',true);i.press(8,'a2',true);
 for(let n=0;n<80;n++)assert.ok(i.sample()&(1<<4));
 i.release(8,'a');assert.ok(i.keys.has(8));
 i.press(8,'keyboard');for(let n=0;n<20;n++)assert.equal(i.sample(),(1<<4)|(1<<8));
 i.release(8,'keyboard');i.release(8,'a2');assert.equal(i.sample(),1<<4);
 i.press(0,'tap',true);i.release(0,'tap');assert.equal(i.sample(),(1<<4)|1);assert.equal(i.sample(),1<<4);
 i.press(9,'cancel',true);i.release(9,'cancel',true);assert.equal(i.sample(),1<<4);
 i.reset();assert.equal(i.sample(),0);
});
test('tablet landscape has separate columns and offline input module; original core unchanged',()=>{
 const css=read('web/style.css'),app=read('web/app.mjs'),html=read('web/index.html'),installer=read('web/offline-installer.mjs');
 assert.match(css,/@media \(orientation:landscape\) and \(min-height:651px\)/);
 assert.ok(css.includes('grid-template-areas:"dpad screen actions" "bar bar bar"'));
 assert.ok(app.includes("window.matchMedia('(orientation:landscape)')"));
 assert.ok(app.includes('const held=joypad.sample();'));
 assert.ok(app.includes("repeat=[0,1,8,9].includes(bit)"));
 assert.ok(app.includes("window.addEventListener('pagehide',resetControls)"));
 assert.ok(app.includes("dbz-hd-tw-pwa-v66"));
 assert.ok(html.includes("ui=v72"));
 const manifest=JSON.parse(read('web/offline-release.json'));
 assert.equal(manifest.version,'v72');
 assert.ok(manifest.files.includes('joypad-input.mjs'));
 assert.deepEqual(manifest.files,JSON.parse(installer.match(/OFFLINE_FILES=Object\.freeze\((\[[^\n]*\])\)/)[1]));
});
