import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,statSync} from 'node:fs';
const root=new URL('../',import.meta.url);
const read=p=>readFileSync(new URL(p,root),'utf8');
const rows=s=>s.split(/\r?\n/).filter(l=>l&&!l.startsWith('#')&&!l.startsWith('key\t')).map(l=>l.split('\t'));
const controls=s=>s.match(/\{[^{}]*\}|\\n/g)||[];
const original=rows(read('translations/zh-hk/zh-hk.tsv'));
const localized=rows(read('web/zh-hk.tsv'));
test('Cantonese table preserves all keys, annotations, control tokens and line breaks',()=>{
 assert.equal(original.length,1495);
 assert.equal(localized.length,original.length);
 let changed=0;
 for(let i=0;i<original.length;i++){
  const [key,source,note]=original[i], [k,text,n]=localized[i];
  assert.equal(k,key);assert.equal(n,note);
  assert.ok(text.trim(),key);
  assert.deepEqual(controls(text),controls(source),key);
  if(/^(unit|attack|object):/.test(key))assert.equal(text,source,key);
  if(text!==source)changed++;
 }
 assert.ok(changed>1100,'substantial localization, not merely a name swap');
 assert.match(read('web/zh-hk.tsv'),/Ming.*CC BY-NC-SA 4\.0/);
 assert.match(read('web/zh-hk.tsv'),/#name\t香港粵語/);
 assert.equal(read('web/zh-tw.tsv'),read('translations/zh-tw/zh-tw.tsv'));
});
test('Cantonese opening, story, combat and prompts contain localized wording',()=>{
 const m=new Map(localized.map(([k,v])=>[k,v]));
 assert.match(m.get('battle:114'),/有咩咁好笑/);
 assert.match(m.get('battle:102'),/我嚟頂住/);
 assert.match(m.get('str2:0x000'),/估唔到你哋/);
 assert.match(m.get('intro:0'),/好耐好耐/);
 assert.match(m.get('battle:13'),/嘅\\n龜波氣功/);
 for(const [k,v] of localized.filter(([k])=>/^intro:/.test(k)))assert.ok([...v].length<=11,k+' '+v);
 assert.match(read('web/app.mjs'),/language==='zh-hk'\?'香港粵語':'台灣繁中'/);
 const release=JSON.parse(read('web/offline-release.json'));
 assert.equal(release.version,'v74');
 assert.equal(release.wasmSize,18561947);
 assert.equal(release.wasmSize,statSync(new URL('web/pkg/ssd_web_bg.wasm',root)).size);
 assert.ok(release.files.includes('zh-hk.tsv'));
 assert.match(read('web/offline-installer.mjs'),/OFFLINE_RELEASE='v74'/);
});
// Read Unicode mappings from the exact TTF bundled inside WASM. No external packages.
function cmapSupports(font){
 let cmap=0;
 for(let i=0;i<font.readUInt16BE(4);i++){
  const p=12+i*16;
  if(font.toString('ascii',p,p+4)==='cmap')cmap=font.readUInt32BE(p+8);
 }
 assert.ok(cmap,'font cmap');
 const tables=[];
 for(let i=0;i<font.readUInt16BE(cmap+2);i++){
  const p=cmap+4+i*8,platform=font.readUInt16BE(p),enc=font.readUInt16BE(p+2);
  if(!(platform===0||platform===3&&(enc===1||enc===10)))continue;
  const offset=cmap+font.readUInt32BE(p+4),format=font.readUInt16BE(offset);
  if(format===4||format===12)tables.push({offset,format});
 }
 assert.ok(tables.length,'Unicode cmap');
 return code=>tables.some(({offset:o,format})=>{
  if(format===12){
   let lo=0,hi=font.readUInt32BE(o+12)-1;
   while(lo<=hi){
    const mid=(lo+hi)>>1,p=o+16+mid*12;
    const start=font.readUInt32BE(p),end=font.readUInt32BE(p+4);
    if(code<start)hi=mid-1;else if(code>end)lo=mid+1;else return font.readUInt32BE(p+8)+code-start!==0;
   }
   return false;
  }
  if(code>0xffff)return false;
  const n=font.readUInt16BE(o+6)/2;
  const end=o+14,start=end+2*n+2,delta=start+2*n,range=delta+2*n;
  for(let j=0;j<n;j++){
   if(code>font.readUInt16BE(end+2*j))continue;
   const first=font.readUInt16BE(start+2*j);
   if(code<first)return false;
   const d=font.readInt16BE(delta+2*j),r=font.readUInt16BE(range+2*j);
   if(!r)return ((code+d)&0xffff)!==0;
   const at=range+2*j+r+2*(code-first);
   if(at+2>o+font.readUInt16BE(o+2))return false;
   const glyph=font.readUInt16BE(at);
   return glyph!==0&&((glyph+d)&0xffff)!==0;
  }
  return false;
 });
}
test('all newly introduced Han characters have glyphs in bundled font',()=>{
 const font=readFileSync(new URL('translations/zh-tw/fonts/LXGWWenKaiTC-Regular.ttf',root));
 const supports=cmapSupports(font);
 const old=new Set([...original.map(r=>r[1]).join('')]);
 const added=[...new Set([...localized.map(r=>r[1]).join('')].filter(c=>/\p{Script=Han}/u.test(c)&&!old.has(c)))];
 const missing=added.filter(c=>!supports(c.codePointAt(0)));
 console.log('New Han glyphs:',added.length,'missing:',missing.join(''));
 assert.deepEqual(missing,[],'Missing Cantonese glyphs');
});
