// Local-only portable archive for the ten DBZ PWA save slots.
// Designed for moving from a Vercel origin to a Cloudflare Pages origin.
// This is a streaming-friendly Blob container, not a game/WASM dependency.
export const BUNDLE_MAGIC='DBZBKP01';
export const BUNDLE_FORMAT='dbz-ssd-save-bundle-v1';
export const BUNDLE_EXTENSION='.dbzbackup';
export const MAX_SLOT_BYTES=24*1024*1024;
export const MAX_THUMB_BYTES=512*1024;
export const MAX_BUNDLE_BYTES=10*(MAX_SLOT_BYTES+MAX_THUMB_BYTES)+65536;
const MAX_HEADER_BYTES=32768;
const encoder=new TextEncoder();
const decoder=new TextDecoder();
const headerPrefix=encoder.encode(BUNDLE_MAGIC);
const checks=(v,message)=>{if(!v)throw Error(message);};
const slotValid=n=>Number.isInteger(n)&&n>=1&&n<=10;
const romValid=s=>typeof s==='string'&&s.length>0&&s.length<=100;
const validRecord=r=>{
  checks(slotValid(r?.slot),'存檔欄位無效');
  checks(Number.isSafeInteger(r.savedAt)&&r.savedAt>0,'存檔時間無效');
  checks(romValid(r.romKey),'ROM／IPS 識別碼無效');
  checks(r.state instanceof Blob&&r.state.size>=1&&r.state.size<=MAX_SLOT_BYTES,
    '存檔資料不完整');
  checks(r.thumbnail==null||
    (r.thumbnail instanceof Blob&&r.thumbnail.size<=MAX_THUMB_BYTES),'縮圖資料無效');
  return r;
};
// No game state is sent anywhere; all file segments stay local Blobs.
export function createMigrationBundle(records,{createdAt=Date.now()}={}){
  checks(Array.isArray(records)&&records.length>=1&&records.length<=10,'找不到可備份的存檔');
  const ordered=[...records].map(validRecord).sort((a,b)=>a.slot-b.slot);
  checks(new Set(ordered.map(r=>r.slot)).size===ordered.length,'存檔欄位重複');
  const slots=ordered.map(r=>({
    slot:r.slot,savedAt:r.savedAt,romKey:r.romKey,
    stateSize:r.state.size,thumbnailSize:r.thumbnail?.size||0,
    thumbnailType:r.thumbnail?.type||''
  }));
  const meta={format:BUNDLE_FORMAT,createdAt,slots};
  const header=encoder.encode(JSON.stringify(meta));
  checks(header.length<=MAX_HEADER_BYTES,'備份檔索引過大');
  const prefix=new Uint8Array(12);
  prefix.set(headerPrefix);
  new DataView(prefix.buffer).setUint32(8,header.length,true);
  const segments=[prefix,header];
  for(const r of ordered){segments.push(r.state);if(r.thumbnail?.size)segments.push(r.thumbnail);}
  const blob=new Blob(segments,{type:'application/octet-stream'});
  checks(blob.size<=MAX_BUNDLE_BYTES,'備份檔過大');
  return {blob,count:ordered.length,slots:slots.map(x=>x.slot)};
}
export async function readMigrationBundle(blob){
  checks(blob instanceof Blob&&blob.size>=14&&blob.size<=MAX_BUNDLE_BYTES,
    '備份檔不存在或大小不正確');
  const prefix=new Uint8Array(await blob.slice(0,12).arrayBuffer());
  checks(prefix.length===12&&
    headerPrefix.every((byte,i)=>prefix[i]===byte),'不是有效的七龍珠完整備份檔');
  const length=new DataView(prefix.buffer).getUint32(8,true);
  checks(length>0&&length<=MAX_HEADER_BYTES&&12+length<=blob.size,'備份索引長度無效');
  let header;
  try{header=JSON.parse(decoder.decode(await blob.slice(12,12+length).arrayBuffer()));}
  catch{throw Error('備份索引不是有效 JSON');}
  checks(header?.format===BUNDLE_FORMAT&&Array.isArray(header.slots)&&
    header.slots.length>0&&header.slots.length<=10,'備份格式不受支援');
  const ids=new Set();
  let cursor=12+length;
  const records=[];
  for(const s of header.slots){
    checks(slotValid(s.slot)&&!ids.has(s.slot),'備份欄位編號無效或重複');
    ids.add(s.slot);
    checks(Number.isSafeInteger(s.savedAt)&&s.savedAt>0&&romValid(s.romKey),
      '備份記錄資訊損壞');
    checks(Number.isInteger(s.stateSize)&&s.stateSize>=1&&s.stateSize<=MAX_SLOT_BYTES,
      '備份進度大小無效');
    checks(Number.isInteger(s.thumbnailSize)&&s.thumbnailSize>=0&&
      s.thumbnailSize<=MAX_THUMB_BYTES,'備份縮圖大小無效');
    checks(typeof s.thumbnailType==='string'&&s.thumbnailType.length<80,
      '備份縮圖格式無效');
    const end=cursor+s.stateSize+s.thumbnailSize;
    checks(end<=blob.size,'備份檔不完整，無法匯入');
    const state=blob.slice(cursor,cursor+s.stateSize,'application/octet-stream');
    cursor+=s.stateSize;
    // Copy the bytes into an independent Blob before IndexedDB persistence (iOS).
    const thumbnail=s.thumbnailSize
      ?new Blob([await blob.slice(cursor,cursor+s.thumbnailSize).arrayBuffer()],{type:s.thumbnailType||'image/webp'}):null;
    cursor+=s.thumbnailSize;
    records.push({slot:s.slot,savedAt:s.savedAt,romKey:s.romKey,state,thumbnail});
  }
  checks(cursor===blob.size,'備份檔含非預期資料');
  return records;
}
