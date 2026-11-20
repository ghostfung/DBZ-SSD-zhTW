// The player's own ROM and optional IPS are private to this PWA's origin.
// IndexedDB stores small local Blobs only; no network upload, no bundled ROM.
// Persisted records are checked again by the existing ROM validator on launch.
const DB_NAME='dbz-hd-tw-private-files-v1',STORE='game-files';
const ALLOWED={rom:8*1024*1024,ips:8*1024*1024};
export const isValidGameFileKind=kind=>Object.hasOwn(ALLOWED,kind);
export function checkGameFileSize(kind,size){
  if(!isValidGameFileKind(kind)||!Number.isFinite(size)||size<1||size>ALLOWED[kind])
    throw Error('不允許的遊戲檔案或檔案大小');
}
export function openGameFilesDB(indexedDBFactory=globalThis.indexedDB){
  if(!indexedDBFactory) return Promise.reject(Error('此瀏覽器不支援離線檔案儲存'));
  return new Promise((resolve,reject)=>{
    const req=indexedDBFactory.open(DB_NAME,1);
    req.onupgradeneeded=()=>{if(!req.result.objectStoreNames.contains(STORE))
      req.result.createObjectStore(STORE,{keyPath:'kind'});};
    req.onsuccess=()=>resolve(req.result);
    req.onerror=()=>reject(req.error||Error('開啟本機儲存失敗'));
    req.onblocked=()=>reject(Error('本機儲存被其他分頁鎖定'));
  });
}
async function transact(kind,mode,act){
  if(!isValidGameFileKind(kind))throw Error('不允許的遊戲檔案類型');
  const db=await openGameFilesDB();
  try {
    return await new Promise((resolve,reject)=>{
      const tx=db.transaction(STORE,mode);
      const request=act(tx.objectStore(STORE));
      request.onsuccess=()=>resolve(request.result);
      request.onerror=()=>reject(request.error||Error('本機檔案操作失敗'));
      tx.onabort=()=>reject(tx.error||Error('本機檔案儲存中止'));
      // A request's success means the transaction may still be committing.
      if(mode==='readwrite'){
        request.onsuccess=null;
        tx.oncomplete=()=>resolve(true);
      }
    });
  }finally{db.close();}
}
export async function saveGameFile(kind,file){
  checkGameFileSize(kind,file?.size);
  const item={kind,blob:new Blob([file],{type:'application/octet-stream'}),
    name:String(file?.name||kind).slice(0,160),size:file.size};
  await transact(kind,'readwrite',store=>store.put(item));
  return {kind,name:item.name,size:item.size};
}
export async function readGameFile(kind){
  const record=await transact(kind,'readonly',store=>store.get(kind));
  if(!record)return null;
  checkGameFileSize(kind,record.blob?.size);
  if(record.size!==record.blob.size)throw Error('本機檔案資料不完整，請重新匯入');
  return record;
}
export async function deleteGameFile(kind){
  await transact(kind,'readwrite',store=>store.delete(kind));
}
