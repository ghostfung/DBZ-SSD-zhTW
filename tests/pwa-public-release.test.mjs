import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {checkGameFileSize,isValidGameFileKind} from '../web/local-game-files.mjs';
const src=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8');

test('private ROM/IPS store allows small game files only',()=>{
 assert.equal(isValidGameFileKind('rom'),true);
 assert.equal(isValidGameFileKind('ips'),true);
 assert.equal(isValidGameFileKind('zip'),false);
 checkGameFileSize('rom',4*1024*1024);
 checkGameFileSize('ips',1024);
 assert.throws(()=>checkGameFileSize('rom',0));
 assert.throws(()=>checkGameFileSize('ips',10*1024*1024));
 assert.throws(()=>checkGameFileSize('hd',1024));
});
test('new ROM explicitly clears the old IPS before local persistence',()=>{
 const app=src('web/app.mjs');
 assert.match(app,/await deleteGameFile\('ips'\)/);
 assert.match(app,/savedRom=null;savedIps=null;ipsInput.value=''/);
 assert.match(app,/await readGameFile\('rom'\)/);
 assert.match(app,/new Uint8Array\(await \(romInput.files\?\.\[0\]\?\?savedRom.blob\)\.arrayBuffer\(\)\)/);
});
test('TW/HK scripts are official and switching does not reload ROM',()=>{
 const tw=src('web/zh-tw.tsv'),hk=src('web/zh-hk.tsv');
 assert.match(tw,/#name\t繁體中文（台灣）/);
 assert.match(hk,/#name\t繁體中文（香港）/);
 assert.match(hk,/龜波氣功/);
 assert.equal(hk,src('translations/zh-hk/zh-hk.tsv'));
 const app=src('web/app.mjs');
 assert.match(app,/game\.load_translation\(await getTranslation\(next\)\)/);
 assert.match(app,/localStorage\.setItem\('dbz-hd-language',language\)/);
});
test('release UI shows game, buttons and language only when playing',()=>{
 const html=src('web/index.html'),css=src('web/style.css');
 for(const id of ['game-files-details','clear-game-files','local-files-status','locale-toggle','hd-toggle']){
  assert.equal([...html.matchAll(new RegExp('id="'+id+'"','g'))].length,1);
 }
 assert.match(css,/\.playing #game > :not\(\.screen\):not\(\.controls\):not\(\.playbar\)/);
 assert.match(css,/grid-template-areas:"dpad screen actions"/);
 assert.match(html,/apple-mobile-web-app-capable/);
 const manifest=JSON.parse(src('web/manifest.webmanifest'));
 assert.equal(manifest.display,'standalone');
 assert.equal(manifest.orientation,'any');
});

test('PWA launch and safe-area controls remain visible, original art is used',()=>{
 const html=src('web/index.html'),css=src('web/style.css');
 for(const id of ['launch-cover','title-cover','setup-files','setup-edit','save','state-file']){
  assert.equal([...html.matchAll(new RegExp('id="'+id+'"','g'))].length,1);
 }
 assert.match(html,/id="title-cover" src="\.\/dbz-title\.webp"/);
 assert.match(html,/apple-touch-icon\.png/);
 assert.match(css,/max\(56px,env\(safe-area-inset-top\)\)/);
 assert.match(css,/max\(66px,calc\(env\(safe-area-inset-left\) \+ 12px\)\)/);
 assert.match(css,/grid-template-areas:"dpad screen actions"/);
 assert.match(css,/position:absolute/);
 const app=src('web/app.mjs');
 assert.match(app,/refreshLaunchCover\(\)/);
 assert.match(app,/globalThis\.hdRestorePromise/);
 assert.match(src('web/hd-import-ui.mjs'),/正在自動保存/);
 assert.match(app,/\$\('state-file'\)\.addEventListener\('change'/);
 assert.match(app,/\$\('save'\)\.addEventListener\('click'/);
 const manifest=JSON.parse(src('web/manifest.webmanifest'));
 assert.equal(manifest.icons[0].src,'./apple-touch-icon.png');
});
