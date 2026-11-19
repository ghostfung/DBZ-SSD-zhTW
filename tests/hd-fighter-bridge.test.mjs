import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const bridge=read('crates/ssd-web/src/lib.rs');
const native=read('crates/ssd-hd/src/lib.rs');
const compositor=read('crates/ssd-web/src/live_fighters.rs');
const ui=read('web/hd-import-ui.mjs');
const app=read('web/app.mjs');

test('HD poses reuse the official OAM sprite-group source-key algorithm',()=>{
 assert.match(native,/art::poses::sprites_in\(log,&view\)/);
 assert.match(native,/art::poses::poses_in\(log,&sprites,&view\)/);
 assert.match(native,/pose\.fighter && !pose\.field && !pose\.bg/);
 assert.match(compositor,/ssd_hd::live_fighter_poses\(log\)/);
 assert.match(compositor,/palette_matches\(entry,&pose\)/);
});
test('HD fighter pixels only override winning OAM sprites and are before native Chinese',()=>{
 assert.match(native,/pub fn oam_ids\(&self\) -> &\[u8\]/);
 assert.match(native,/pixel\.layer == screen::Layer::Obj \{ pixel\.oam \}/);
 assert.match(compositor,/pose\.oam_bits & \(1_u128<<owner\)\)\!=0/);
 assert.match(compositor,/let owner=oam_ids\[p\]/);
 assert.ok(bridge.indexOf('self.live_fighters.composite(')>0);
 assert.ok(bridge.indexOf('self.live_fighters.composite(')<bridge.indexOf('self.hd_renderer.paint_native_translation('));
 assert.match(bridge,/pub fn hd_fighter_pixels\(&self\) -> u32/);
 assert.match(bridge,/render_fighter_beneath\(log,hidden/);
 assert.match(bridge,/set_fighter_hd_enabled\(&mut self, enabled: bool\)/);
 assert.match(native,/art::poses::fighter_candidates\(&sprites\)/);
 assert.match(read('web/index.html'),/id="fighter-hd-toggle"/);
});
test('fighter catalogue and art come only from local ZIP and exclude missing palette',()=>{
 assert.match(ui,/readFighterCatalog\(source\)/);
 assert.match(ui,/CustomEvent\('hd-fighter-catalog-ready'/);
 assert.match(app,/liveFighters\.setCatalog\(/);
 assert.match(app,/liveFighters\.pulse\(\)/);
 assert.match(app,/liveFighters\.detach\(\)/);
 assert.doesNotMatch(app,/fetch\(.+class2/);
 assert.match(read('web/hd-fighter-live.mjs'),/this\.decoder\(assets,match\.art\)/);
 assert.match(read('web/hd-fighter-index.mjs'),/!entry\.luma && colourCount>=2 && hasFile\(art\)/);
});
