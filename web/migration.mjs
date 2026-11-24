import {listSaveSlots,SLOT_COUNT} from './save-slots.mjs';
import {createMigrationBundle} from './migration-bundle.mjs';
const $=id=>document.getElementById(id);
let backup=null,scanning=false,objectUrl=null;
function renderSlots(all,failed=false){
  const bySlot=new Map(all.map(x=>[x.slot,x]));
  const grid=$('slots');
  grid.replaceChildren();
  for(let i=1;i<=SLOT_COUNT;i++){
    const record=bySlot.get(i),cell=document.createElement('div');
    const label=document.createElement('strong'),detail=document.createElement('span');
    label.textContent='Slot '+String(i).padStart(2,'0');
    cell.className='slot';
    cell.dataset.state=failed?'unknown':record?.corrupt?'corrupt':record?'valid':'empty';
    detail.textContent=failed?'無法檢查':record?.corrupt?'資料損壞':record?'已存檔 · '+new Date(record.savedAt).toLocaleString('zh-TW'):'空白';
    cell.append(label,detail);
    grid.append(cell);
  }
}
async function scan(){
  if(scanning)return;
  scanning=true; backup=null; $('export').disabled=true; $('export').hidden=true;
  $('backup-panel').hidden=true; $('restore-step').hidden=true; $('no-save-note').hidden=true;
  try{
    const all=await listSaveSlots();
    const valid=all.filter(x=>!x.corrupt);
    const bad=all.length-valid.length;
    $('scan-note').hidden=true;
    if(!all.length){
      $('no-save-note').hidden=false;
      return;
    }
    $('backup-panel').hidden=false;
    renderSlots(all);
    $('status').textContent='找到 '+valid.length+'／10 格有效存檔'+(bad?'；另有 '+bad+' 格資料損壞':'');
    $('result').textContent=bad?'損壞的欄位無法備份，請保留舊 PWA 資料。':'';
    if(valid.length){
      backup=createMigrationBundle(valid);
      $('export').hidden=false;
      $('export').disabled=false;
      $('restore-step').hidden=false;
    }
  }catch(error){
    $('scan-note').hidden=true;
    $('backup-panel').hidden=false;
    renderSlots([],true);
    $('status').textContent='讀取或打包舊版存檔失敗：'+(error?.message||error);
    $('result').textContent='請勿刪除舊版程式或清除網站資料。';
  }finally{scanning=false;}
}
$('export').addEventListener('click',()=>{
  if(!backup)return;
  try{
    if(objectUrl)URL.revokeObjectURL(objectUrl);
    objectUrl=URL.createObjectURL(backup.blob);
    const link=document.createElement('a');
    link.href=objectUrl;link.download='dbz-ssd-all-slots.dbzbackup';
    document.body.append(link);link.click();link.remove();
    $('result').textContent='已產生 '+backup.count+' 格存檔備份。請到「檔案」確認已保存 dbz-ssd-all-slots.dbzbackup，再依下方說明前往新網站還原。';
  }catch(error){
    $('result').textContent='備份未成功：'+(error?.message||error)+'。請勿刪除舊圖示。';
  }
});
void scan();
