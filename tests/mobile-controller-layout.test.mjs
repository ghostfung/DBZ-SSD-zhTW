import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const html=readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
const css=readFileSync(new URL('../web/style.css',import.meta.url),'utf8');

test('controller and utility actions immediately follow the canvas before lengthy HD diagnostics',()=>{
 const game=html.slice(html.indexOf('<section id="game"'),html.indexOf('</section>',html.indexOf('<section id="game"')));
 const index=x=>game.indexOf(x);
 assert.ok(index('id="screen"')<index('class="controls"'));
 assert.ok(index('class="controls"')<index('class="utility"'));
 assert.ok(index('class="utility"')<index('id="game-diagnostics"'));
 assert.ok(index('id="game-diagnostics"')<index('id="frame-perf"'));
 assert.ok(index('id="game-diagnostics"')<index('id="hd-phase-perf"'));
 assert.ok(index('id="game-diagnostics"')<index('id="hd-fighter-info"'));
 assert.match(css,/\.controls\{[^}]*margin:9px auto 8px/);
});

test('diagnostics are collapsed by default and toggles retain original IDs',()=>{
 const details=html.match(/<details id="game-diagnostics"[^>]*>/)?.[0]||'';
 assert.ok(details);
 assert.doesNotMatch(details,/\sopen(?:\s|=|>)/);
 const ids=['hd-toggle','fighter-hd-toggle','portrait-hd-toggle',
  'special-hd-toggle','map-hd-toggle','card-hd-toggle','mode7-hd-toggle',
  'frame-perf','hd-phase-perf','translation-card','sound'];
 for(const id of ids)assert.equal(
   [...html.matchAll(new RegExp('id="'+id+'"','g'))].length,1,
   'existing control/status '+id+' must be unique');
});

test('touch controls stay interactive without changed bits or additional JS listeners',()=>{
 for(const [bit,label] of [[4,'上'],[5,'下'],[6,'左'],[7,'右']]){
   assert.match(html,new RegExp('data-bit="'+bit+'"[^>]*aria-label="'+label+'"'));
 }
 for(const bit of [0,1,2,3,8,9]){
   assert.equal([...html.matchAll(new RegExp('data-bit="'+bit+'"','g'))].length,1);
 }
 const app=readFileSync(new URL('../web/app.mjs',import.meta.url),'utf8');
 assert.match(app,/document\.querySelectorAll\('button\[data-bit\]'\)/);
 assert.match(app,/setPointerCapture\(e\.pointerId\)/);
 assert.match(app,/lostpointercapture/);
});
