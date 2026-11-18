// Read the ZIP central directory via File.slice; do not expand a 462 MiB archive in memory.
import {sha256File} from './sha256-stream.mjs';

export const OFFICIAL_PACK = Object.freeze({
  name:'hd-pack.zip', version:'v0.3.9',
  bytes:484121119,
  sha256:'e55cb05d4cb98904d4893b3597a90b7259893c34797f3b8e5b5c1cd0b621d4cc'
});
const EOCD=0x06054b50, CD=0x02014b50, LOCAL=0x04034b50;
const le=(v,offset)=>v.getUint16(offset,true);
const le32=(v,offset)=>v.getUint32(offset,true);

function safeName(name){
  return name && !name.includes('\\') && !name.includes('\0') &&
    !name.startsWith('/') && !/^[A-Za-z]:/.test(name) &&
    !name.split('/').some(p=>p==='..') &&
    !name.split('/').some(p=>p==='.');
}

export async function inspectZip(file) {
  if(!file?.size || file.size<22)throw Error('不是有效的 ZIP 檔案');
  const from=Math.max(0,file.size-65557);
  const tail=new Uint8Array(await file.slice(from).arrayBuffer());
  const tv=new DataView(tail.buffer,tail.byteOffset,tail.byteLength);
  let at=-1;
  for(let i=tail.byteLength-22;i>=0;i--){
    if(le32(tv,i)===EOCD && i+22+le(tv,i+20)===tail.byteLength) {at=i;break;}
  }
  if(at<0)throw Error('找不到 ZIP 結尾目錄，檔案可能不完整');
  if(le(tv,at+4)!==0||le(tv,at+6)!==0)throw Error('不支援分卷 ZIP');
  const count=le(tv,at+10),size=le32(tv,at+12),offset=le32(tv,at+16);
  if(count===0xffff||size===0xffffffff||offset===0xffffffff)throw Error('ZIP64 尚未支援');
  if(count!==le(tv,at+8)||!count||count>65000)throw Error('ZIP 目錄筆數異常');
  if(size>100*1024*1024 || offset+size>from+at)throw Error('ZIP 中央目錄位置或長度錯誤');
  const data=new Uint8Array(await file.slice(offset,offset+size).arrayBuffer());
  const view=new DataView(data.buffer,data.byteOffset,data.byteLength);
  const entries=[];let i=0;
  const decoder=new TextDecoder('utf-8',{fatal:true});
  for(let n=0;n<count;n++){
    if(i+46>data.byteLength||le32(view,i)!==CD)throw Error('ZIP 中央目錄資料損壞');
    const flags=le(view,i+8),method=le(view,i+10);
    const crc32=le32(view,i+16),packed=le32(view,i+20),unpacked=le32(view,i+24);
    const nameSize=le(view,i+28),extra=le(view,i+30),comment=le(view,i+32),start=le32(view,i+42);
    const end=i+46+nameSize+extra+comment;
    if(end>data.length)throw Error('ZIP 檔名長度錯誤');
    if((flags&1)!==0)throw Error('不接受加密 ZIP');
    if(method!==0&&method!==8)throw Error('ZIP 包含不支援的壓縮格式 '+method);
    if(packed===0xffffffff||unpacked===0xffffffff||start===0xffffffff)throw Error('目前不支援 ZIP64 格式');
    let name;
    try{name=decoder.decode(data.subarray(i+46,i+46+nameSize));}
    catch{throw Error('ZIP 包含無效的 UTF-8 檔名');}
    if(!safeName(name))throw Error('ZIP 檔案路徑不安全：'+name);
    if(start>=file.size)throw Error('ZIP 子檔案位置超出檔尾');
    entries.push({name,method,packed,unpacked,crc32,start});
    i=end;
  }
  if(i!==size)throw Error('ZIP 中央目錄大小不符');
  if(new Set(entries.map(x=>x.name)).size!==entries.length)throw Error('ZIP 有重複的檔名');
  const names=entries.map(x=>x.name);
  const manifest=names.find(x=>x.toLowerCase().endsWith('hdpack.toml'))??null;
  const index=names.find(x=>x.toLowerCase().endsWith('index.bin'))??null;
  const art=names.filter(x=>/\.(png|webp)$/i.test(x)).length;
  return {count,entries,manifest,index,imageCount:art,totalUnpacked:entries.reduce((v,x)=>v+x.unpacked,0)};
}

function crc32bytes(buffer){
  let c=0xffffffff;
  for(const b of buffer){
    c^=b;
    for(let k=0;k<8;k++)c=(c>>>1)^((c&1)?0xedb88320:0);
  }
  return (c^0xffffffff)>>>0;
}

export async function extractZipEntry(file,entry,{maxBytes=16*1024*1024}={}) {
  if(!entry || entry.unpacked>maxBytes)throw Error('該素材超出單檔讀取上限');
  const raw=new DataView(await file.slice(entry.start,entry.start+30).arrayBuffer());
  if(raw.byteLength!==30||le32(raw,0)!==LOCAL)throw Error('ZIP 子檔案標頭損壞');
  const at=entry.start+30+le(raw,26)+le(raw,28);
  if(at+entry.packed>file.size)throw Error('ZIP 子檔案內容超出檔尾');
  const part=file.slice(at,at+entry.packed);
  let stream=part.stream();
  if(entry.method===8){
    if(typeof DecompressionStream!=='function')throw Error('此瀏覽器不能解壓 DEFLATE');
    stream=stream.pipeThrough(new DecompressionStream('deflate-raw'));
  }
  const reader=stream.getReader();
  const chunks=[];let total=0;
  try {
    for(;;) {
      const {done,value}=await reader.read();
      if(done)break;
      total+=value.length;
      if(total>maxBytes || total>entry.unpacked)throw Error('解壓內容超過預期大小');
      chunks.push(value);
    }
  } finally {reader.releaseLock();}
  if(total!==entry.unpacked)throw Error('ZIP 解壓長度不符');
  const output=new Uint8Array(total);let cursor=0;
  for(const c of chunks){output.set(c,cursor);cursor+=c.length;}
  if(crc32bytes(output)!==entry.crc32)throw Error('ZIP 子檔案 CRC32 校驗失敗');
  return output;
}

export async function verifyOfficialPack(file,{onProgress=()=>{},signal}={}) {
  if(file.size!==OFFICIAL_PACK.bytes)throw Error(`HD 美術包大小不符：需要 ${OFFICIAL_PACK.bytes.toLocaleString('en-US')} bytes`);
  const zip=await inspectZip(file);
  if(signal?.aborted)throw new DOMException('已取消驗證','AbortError');
  const digest=await sha256File(file,{onProgress,signal});
  if(digest!==OFFICIAL_PACK.sha256)throw Error('SHA-256 不一致。這不是官方 v0.3.9 原版素材包，或檔案已損壞。');
  return {name:OFFICIAL_PACK.name,version:OFFICIAL_PACK.version,bytes:file.size,sha256:digest,
    count:zip.count,imageCount:zip.imageCount,manifest:zip.manifest,index:zip.index,
    entries:zip.entries};
}
