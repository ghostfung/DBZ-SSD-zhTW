import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const src=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const css=src('web/style.css');
const html=src('web/index.html');
const final=css.slice(css.lastIndexOf('/* v50 — thumb-safe'));

test('portrait shifts controls and toolbar below the game while budgeting for audio recovery',()=>{
  assert.match(final,/@media \(orientation:portrait\)/);
  assert.match(final,/max-height:min\(calc\(100dvh - 430px\),calc\(100vw \* 478 \/ 512\)\)/);
  assert.match(final,/\.playing #game \.controls\{\s*margin:clamp\(12px,2\.3dvh,24px\) auto/);
  assert.match(final,/\.playing #game \.playbar\{\s*margin:clamp\(7px,1\.4dvh,14px\) auto 0/);
  assert.match(css,/\.playing #game \.playbar\{display:grid;grid-template-columns:repeat\(3,minmax\(0,1fr\)\)/);
  assert.match(html,/id="sound" type="button" hidden/);
});

test('landscape moves BOTH game controller groups down without covering sound/settings',()=>{
  const landscape=final.slice(final.indexOf('@media (orientation:landscape)'));
  assert.match(landscape,/\.playing #game \.dpad,\s*\.playing #game \.actions\{\s*align-self:end/);
  assert.match(landscape,/margin-bottom:max\(14px,env\(safe-area-inset-bottom\)\)/);
  assert.match(landscape,/\.playing #game \.dpad\{transform:translateX\(2px\)\}/);
  assert.match(landscape,/\.playing #game \.actions\{transform:none\}/);
  assert.match(landscape,/\.playing #game \.playbar\{\s*position:absolute;\s*top:max\(14px,calc\(env\(safe-area-inset-top\) \+ 8px\)\)/);
  // Regression for the fourth toolbar row when sound-reconnect is visible:
  // at 568×320 and 852×393, even the tallest action cluster starts
  // below the toolbar (14px top + 4×28px rows + 3×3px gaps).
  for(const [width,height] of [[568,320],[852,393],[932,430]]){
    const actionHeight=3*Math.min(width*0.057,49)+6;
    const actionsTop=height-6-14-actionHeight;
    const toolbarBottom=14+4*28+3*3;
    assert.ok(actionsTop>toolbarBottom+10,`toolbar overlap at ${width}×${height}`);
  }
});

test('layout-only update leaves original SNES controls, language and audio modules intact',()=>{
  for(const bit of [0,1,2,3,4,5,6,7,8,9]){
    assert.equal([...html.matchAll(new RegExp('data-bit="'+bit+'"','g'))].length,1);
  }
  assert.match(html,/style\.css\?build=v50&ui=v59/);
  assert.match(html,/app\.mjs\?build=v53&ui=v63/);
  assert.match(src('web/app.mjs'),/reconnectFromGesture\(\)/);
  assert.match(src('crates/ssd-web/src/lib.rs'),/dbz-hd-tw-pwa-v53/);
});
