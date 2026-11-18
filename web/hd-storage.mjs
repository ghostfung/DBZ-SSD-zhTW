// OPFS-backed HD ZIP cache. Store small independent parts, not one 462 MiB writable file.
// No ROM/IPS or art contents are sent to a server.
import {OFFICIAL_PACK} from './hd-pack.mjs';

const DIRECTORY='dbz-ssd-private';
const LEGACY_FILE='hd-pack-v0.3.9.zip';
const LEGACY_META='hd-pack-v0.3.9.json';
const PARTS_DIR='hd-pack-v0.3.9-parts-v2';
const META='hd-pack-v0.3.9-v2.json';
const VERSION=2;
export const PART_SIZE=2*1024*1024;
export function opfsAvailable(){return Boolean(globalThis.navigator?.storage?.getDirectory);}

async function directory(create=false){
  if(!opfsAvailable())throw Error('目前瀏覽器不支援 OPFS 本機儲存。');
  const root=await navigator.storage.getDirectory();
  return root.getDirectoryHandle(DIRECTORY,{create});
}
const nameOf = i => 'part-'+String(i).padStart(4,'0')+'.bin';
const expectedSize = (i,bytes) => Math.min(PART_SIZE,bytes-i*PART_SIZE);
const yieldBrowser = () => new Promise(resolve=>setTimeout(resolve,0));

async function writeText(dir,name,text){
  const handle=await dir.getFileHandle(name,{create:true});
  const out=await handle.createWritable({keepExistingData:false});
  try{await out.write(new Blob([text],{type:'application/json'}));await out.close();}
  catch(error){await out.abort().catch(()=>{});throw error;}
}

/** Blob-compatible random-access view over OPFS Files, without merging 462 MiB in memory. */
export class SegmentedBlob {
  constructor(parts){
    this.parts=parts;
    this.size=parts.reduce((sum,p)=>sum+p.size,0);
    this.type='application/zip';
  }
  slice(start=0,end=this.size,type=''){
    const at=Math.max(0,Math.min(this.size,start<0?this.size+start:start));
    const until=Math.max(at,Math.min(this.size,end<0?this.size+end:end));
    let offset=0;
    const chunks=[];
    for(const part of this.parts){
      const overlapStart=Math.max(at,offset);
      const overlapEnd=Math.min(until,offset+part.size);
      if(overlapStart<overlapEnd)chunks.push(part.slice(overlapStart-offset,overlapEnd-offset));
      offset+=part.size;
      if(offset>=until)break;
    }
    return new Blob(chunks,{type});
  }
}

/** Write a Blob as bounded OPFS files. Exported for small synthetic regression tests. */
export async function writeSegmentedParts(parts,file,{chunkBytes=PART_SIZE,onProgress=()=>{},signal}={}){
  const count=Math.ceil(file.size/chunkBytes);
  for(let index=0;index<count;index++){
    const start=index*chunkBytes;
    const end=Math.min(file.size,start+chunkBytes);
    try{
      if(signal?.aborted)throw new DOMException('已取消儲存','AbortError');
      const handle=await parts.getFileHandle(nameOf(index),{create:true});
      const writer=await handle.createWritable({keepExistingData:false});
      try{
        await writer.write(file.slice(start,end));
        await writer.close();
      }catch(err){await writer.abort().catch(()=>{});throw err;}
      const stored=await handle.getFile();
      if(stored.size!==end-start)throw Error('分段寫入大小不符');
      onProgress(end,file.size);
      await yieldBrowser();
    }catch(error){
      throw Error('第 '+(index+1)+'／'+count+' 段寫入失敗：'+(error?.message||String(error)));
    }
  }
  return count;
}

export async function saveVerifiedHdPack(file,verified,{onProgress=()=>{},signal}={}){
  if(verified?.sha256!==OFFICIAL_PACK.sha256||file.size!==OFFICIAL_PACK.bytes)
    throw Error('必須先完成官方 HD 美術包 SHA-256 驗證');
  if(!opfsAvailable())throw Error('Safari 沒有提供 OPFS 功能，請保留檔案在「檔案」App。');
  const estimate=await navigator.storage.estimate?.();
  if(estimate?.quota && estimate?.usage!==undefined &&
     estimate.quota-estimate.usage < file.size+8*1024*1024)
    throw Error('瀏覽器估算的剩餘儲存空間不足以保存 HD ZIP；請保留原始檔案。');

  const dir=await directory(true);
  // Never indicate success until all parts and their metadata are written.
  await dir.removeEntry(META).catch(()=>{});
  const parts=await dir.getDirectoryHandle(PARTS_DIR,{create:true});
  const count=Math.ceil(file.size/PART_SIZE);
  try{
    await writeSegmentedParts(parts,file,{chunkBytes:PART_SIZE,onProgress,signal});
    await writeText(dir,META,JSON.stringify({
      version:VERSION,name:OFFICIAL_PACK.name,bytes:OFFICIAL_PACK.bytes,
      sha256:OFFICIAL_PACK.sha256,partSize:PART_SIZE,count,
      savedAt:new Date().toISOString(),imageCount:verified.imageCount,
      entries:verified.count
    }));
    // All parts can be opened independently; no 462 MiB Blob construction needed.
    const restored=await readSavedHdPack();
    if(!restored)throw Error('分段寫入完成，但重新讀取未成功');
    return restored.file;
  }catch(error){
    await dir.removeEntry(META).catch(()=>{});
    await dir.removeEntry(PARTS_DIR,{recursive:true}).catch(()=>{});
    throw Error('HD 素材包本機儲存失敗：'+(error?.message||String(error)));
  }
}

export async function readSavedHdPack(){
  if(!opfsAvailable())return null;
  try{
    const dir=await directory(false);
    const data=JSON.parse(await (await dir.getFileHandle(META)).getFile().then(f=>f.text()));
    if(data.version!==VERSION||data.bytes!==OFFICIAL_PACK.bytes||
       data.sha256!==OFFICIAL_PACK.sha256||data.partSize!==PART_SIZE||
       data.count!==Math.ceil(OFFICIAL_PACK.bytes/PART_SIZE))return null;
    const partDir=await dir.getDirectoryHandle(PARTS_DIR);
    const chunks=[];
    for(let i=0;i<data.count;i++){
      const part=await (await partDir.getFileHandle(nameOf(i))).getFile();
      if(part.size!==expectedSize(i,data.bytes))return null;
      chunks.push(part);
    }
    const archive=new SegmentedBlob(chunks);
    return archive.size===OFFICIAL_PACK.bytes?{file:archive,metadata:data}:null;
  }catch{
    // Continue to read a valid cache written by v5.1.
    try{
      const dir=await directory(false);
      const data=JSON.parse(await (await dir.getFileHandle(LEGACY_META)).getFile().then(f=>f.text()));
      if(data.version!==1||data.bytes!==OFFICIAL_PACK.bytes||
        data.sha256!==OFFICIAL_PACK.sha256)return null;
      const file=await (await dir.getFileHandle(LEGACY_FILE)).getFile();
      return file.size===OFFICIAL_PACK.bytes?{file,metadata:data}:null;
    }catch{return null;}
  }
}

export async function clearSavedHdPack(){
  if(!opfsAvailable())return;
  try{
    const dir=await directory(false);
    await dir.removeEntry(META).catch(()=>{});
    await dir.removeEntry(PARTS_DIR,{recursive:true}).catch(()=>{});
    await dir.removeEntry(LEGACY_META).catch(()=>{});
    await dir.removeEntry(LEGACY_FILE).catch(()=>{});
  }catch{}
}
