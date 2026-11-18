// Safari OPFS cache. Imported ROM/IPS are never saved by this module.
// The package is only marked installed AFTER every byte has been written successfully.
import {OFFICIAL_PACK} from './hd-pack.mjs';

const DIRECTORY='dbz-ssd-private';
const FILE='hd-pack-v0.3.9.zip';
const META='hd-pack-v0.3.9.json';
const META_VERSION=1;
export function opfsAvailable(){return Boolean(globalThis.navigator?.storage?.getDirectory);}

async function directory(create=false){
  if(!opfsAvailable())throw Error('目前瀏覽器不支援 OPFS 本機儲存。可繼續使用檔案匯入，無須上傳。');
  const root=await navigator.storage.getDirectory();
  return root.getDirectoryHandle(DIRECTORY,{create});
}

async function writeSmallFile(dir,name,text){
  const handle=await dir.getFileHandle(name,{create:true});
  const stream=await handle.createWritable();
  try {await stream.write(text);await stream.close();}
  catch(e){await stream.abort().catch(()=>{});throw e;}
}

export async function saveVerifiedHdPack(file,verified,{onProgress=()=>{},signal}={}){
  if(verified?.sha256!==OFFICIAL_PACK.sha256||file.size!==OFFICIAL_PACK.bytes)
    throw Error('必須先完成官方 HD 素材包 SHA-256 驗證');
  if(!opfsAvailable())throw Error('Safari 沒有提供 OPFS 功能，請使用當前檔案匯入模式');
  const estimate=await navigator.storage.estimate?.();
  if(estimate?.quota && estimate?.usage!==undefined &&
     estimate.quota-estimate.usage < file.size+8*1024*1024)
    throw Error('瀏覽器可用儲存空間不足，請保留 ZIP 在「檔案」App，或清理空間後再試');
  const dir=await directory(true);
  await dir.removeEntry(META).catch(()=>{});
  const target=await dir.getFileHandle(FILE,{create:true});
  const writable=await target.createWritable({keepExistingData:false});
  let written=0,finished=false;
  try {
    while(written<file.size){
      if(signal?.aborted)throw new DOMException('已取消儲存','AbortError');
      const end=Math.min(written+2*1024*1024,file.size);
      await writable.write(new Uint8Array(await file.slice(written,end).arrayBuffer()));
      written=end;
      onProgress(written,file.size);
    }
    await writable.close();
    finished=true;
    // A size check catches truncated writes. The hash was verified before write.
    const saved=await (await target.getFile());
    if(saved.size!==file.size)throw Error('本機保存後檔案大小不符');
    await writeSmallFile(dir,META,JSON.stringify({
      version:META_VERSION,sha256:OFFICIAL_PACK.sha256,bytes:OFFICIAL_PACK.bytes,
      name:OFFICIAL_PACK.name,savedAt:new Date().toISOString(),
      count:verified.count,imageCount:verified.imageCount
    }));
    return saved;
  }catch(error){
    if(!finished)await writable.abort().catch(()=>{});
    await dir.removeEntry(FILE).catch(()=>{});
    await dir.removeEntry(META).catch(()=>{});
    throw error;
  }
}

export async function readSavedHdPack(){
  if(!opfsAvailable())return null;
  try{
    const dir=await directory(false);
    const handle=await dir.getFileHandle(META);
    const json=JSON.parse(await (await handle.getFile()).text());
    if(json.version!==META_VERSION||json.sha256!==OFFICIAL_PACK.sha256||json.bytes!==OFFICIAL_PACK.bytes)return null;
    const file=await (await dir.getFileHandle(FILE)).getFile();
    if(file.size!==OFFICIAL_PACK.bytes)return null;
    return {file,metadata:json};
  }catch{return null;}
}

export async function clearSavedHdPack(){
  if(!opfsAvailable())return;
  try{
    const dir=await directory(false);
    await dir.removeEntry(META).catch(()=>{});
    await dir.removeEntry(FILE).catch(()=>{});
  }catch{}
}
