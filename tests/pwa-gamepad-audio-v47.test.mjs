import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');

test('Super Famicom diamond uses true X/Y/A/B input bits with original colors',()=>{
  const html=source('web/index.html'),css=source('web/style.css');
  for(const [bit,className,label,color] of [
    [9,'sfc-x','X','#366bc5'],[1,'sfc-y','Y','#248e51'],
    [8,'sfc-a','A','#df4e38'],[0,'sfc-b','B','#edc33d']
  ]){
    assert.match(html,new RegExp('data-bit="'+bit+'" class="'+className+'"'));
    assert.match(css,new RegExp('\\.'+className+'\\{[^}]*'+color));
    assert.equal([...html.matchAll(new RegExp('data-bit="'+bit+'"','g'))].length,1);
  }
  assert.match(css,/\.sfc-x\{grid-column:2;grid-row:1/);
  assert.match(css,/\.sfc-y\{grid-column:1;grid-row:2/);
  assert.match(css,/\.sfc-a\{grid-column:3;grid-row:2/);
  assert.match(css,/\.sfc-b\{grid-column:2;grid-row:3/);
  assert.match(css,/grid-template-columns:repeat\(3,min\(5\.15vw,46px\)\)/);
  assert.match(css,/transform:translate\(2px,16px\)/);
  assert.match(css,/grid-template-columns:repeat\(3,min\(5\.7vw,49px\)\)/);
  assert.match(css,/align-self:center;justify-self:center;transform:translateY\(16px\)/);
  assert.match(css,/width:clamp\(152px,19\.5vw,172px\)/);
});

test('file-state restore resumes Safari sound and offers manual recovery on denial',()=>{
  const html=source('web/index.html'),app=source('web/app.mjs'),audio=source('web/audio.mjs');
  assert.equal([...html.matchAll(/id="sound"/g)].length,1);
  assert.match(html,/id="sound" type="button" hidden/);
  assert.match(app,/async function restoreAudioAfterInterruption/);
  assert.match(app,/game\.load_state\(new Uint8Array/);
  assert.match(app,/const restored=await restoreAudioAfterInterruption\(\)/);
  assert.match(app,/game\.set_audio_rate\(sound\.sampleRate\)/);
  assert.match(app,/document\.addEventListener\('visibilitychange'/);
  assert.match(audio,/async resumeAfterInterruption\(\)/);
  assert.doesNotMatch(app,/window\.alert\('讀取存檔成功'\)/);
});

test('returning launcher shows measured HD restore percent and launch stages',()=>{
  const html=source('web/index.html'),css=source('web/style.css'),app=source('web/app.mjs');
  assert.match(app,/const compactLaunch=\$\('setup'\)\.classList\.contains\('launch-ready'\)/);
  assert.match(app,/document\.addEventListener\('hd-restore-progress'/);
  assert.match(app,/activeHdRestoreReporter=\(\{done,total,percent\}\)=>/);
  assert.match(app,/button\.textContent=`HD 驗證 · \$\{percent\}%`/);
  assert.match(app,/啟動步驟 \$\{stageIndex\}\/\$\{stageCount\}/);
  assert.match(source('web/hd-import-ui.mjs'),/detail:\{done:n,total\}/);
  assert.match(css,/#setup\.launch-ready #setup-files\{display:none!important\}/);
  assert.match(html,/id="hd-verify"/);
  assert.match(html,/id="hd-progress"/);
});

test('public home footer removed but CC attribution remains in a separate accessible page',()=>{
  const html=source('web/index.html'),credits=source('web/credits.html');
  assert.doesNotMatch(html,/class="footer"/);
  assert.match(html,/href="\.\/credits\.html"/);
  assert.match(credits,/CC BY-NC-SA 4\.0/);
  assert.match(credits,/https:\/\/github\.com\/lming888\/DBZ-SSD-zhTW/);
});
