// Ten private save-state slots for this PWA origin. No network, ROM, IPS or HD assets are stored here.
// The emulator's .sav bytes remain unchanged; only metadata and a small thumbnail are added.
const DB_NAME='dbz-hd-tw-save-slots-v1';
const STORE='slots';
export const SLOT_COUNT=10;
export const MAX_STATE_BYTES=24*1024*1024;
export const MAX_THUMB_BYTES=512*1024;

export function assertSlotNumber(slot){
  if(!Number.isInteger(slot)||slot<1||slot>SLOT_COUNT)
    throw Error('存檔欄位必須介於 1 至 10');
  return slot;
}
export function validStateBytes(bytes){
  if(!(bytes instanceof Uint8Array)||bytes.byteLength<1||bytes.byteLength>MAX_STATE_BYTES)
    throw Error('存檔格式或大小不正確');
  return bytes;
}
function validMetadata(record){
  if(!record||!Number.isSafeInteger(record.savedAt)||record.savedAt<=0||
     typeof record.romKey!=='string'||!record.romKey||record.romKey.length>100)
    throw Error('存檔資料不完整');
  if(!(record.state instanceof Blob)||record.state.size<1||record.state.size>MAX_STATE_BYTES)
    throw Error('存檔資料已損壞');
  if(record.thumbnail!==null&&record.thumbnail!==undefined&&
    (!(record.thumbnail instanceof Blob)||record.thumbnail.size>MAX_THUMB_BYTES))
    throw Error('存檔縮圖已損壞');
  return record;
}
export function openSlotDB(factory=globalThis.indexedDB){
  if(!factory) return Promise.reject(Error('此瀏覽器不支援本機存檔'));
  return new Promise((resolve,reject)=>{
    const req=factory.open(DB_NAME,1);
    req.onupgradeneeded=()=>{
      if(!req.result.objectStoreNames.contains(STORE))
        req.result.createObjectStore(STORE,{keyPath:'slot'});
    };
    req.onsuccess=()=>resolve(req.result);
    req.onerror=()=>reject(req.error||Error('無法開啟本機存檔'));
    req.onblocked=()=>reject(Error('本機存檔資料庫被其他視窗佔用，請關閉其他分頁'));
  });
}
async function transact(mode,requestFn){
  const db=await openSlotDB();
  try{
    return await new Promise((resolve,reject)=>{
      let result;
      const tx=db.transaction(STORE,mode);
      tx.oncomplete=()=>resolve(result);
      tx.onabort=()=>reject(tx.error||Error('存檔交易已取消'));
      tx.onerror=()=>reject(tx.error||Error('本機存檔讀寫失敗'));
      let request;
      try{request=requestFn(tx.objectStore(STORE));}
      catch(error){try{tx.abort();}catch{} reject(error);return;}
      request.onsuccess=()=>{result=request.result;};
      request.onerror=()=>reject(request.error||Error('本機存檔讀寫失敗'));
    });
  }finally{db.close();}
}
export async function listSaveSlots(){
  const records=await transact('readonly',store=>store.getAll());
  return records.filter(record=>Number.isInteger(record?.slot)&&record.slot>=1&&record.slot<=SLOT_COUNT)
    .map(record=>{
      try{return validMetadata(record);}
      catch{return {slot:record.slot,corrupt:true};}
    });
}
export async function readSaveSlot(slot){
  const record=await transact('readonly',store=>store.get(assertSlotNumber(slot)));
  return record?validMetadata(record):null;
}
export async function writeSaveSlot(slot,bytes,{romKey,thumbnail=null,savedAt=Date.now()}={}){
  assertSlotNumber(slot);
  validStateBytes(bytes);
  const record={slot,savedAt,romKey,thumbnail,
    state:new Blob([bytes],{type:'application/octet-stream'})};
  validMetadata(record);
  // A single put transaction replaces the state, timestamp and thumbnail together.
  // The write is considered successful only when the whole transaction commits.
  await transact('readwrite',store=>store.put(record));
  const check=await readSaveSlot(slot);
  if(!check||check.savedAt!==savedAt||check.state.size!==bytes.byteLength)
    throw Error('存檔寫入後驗證失敗');
  return check;
}
