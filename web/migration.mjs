import {listSaveSlots} from './save-slots.mjs';
import {createMigrationBundle} from './migration-bundle.mjs';
const $=id=>document.getElementById(id);
let records=[];
async function scan(){
  try{
    const all=await listSaveSlots();
    records=all.filter(x=>!x.corrupt);
    const bad=all.filter(x=>x.corrupt).length;
    $('status').textContent=records.length
      ?'找到 '+records.length+' 格有效進度'+(bad?'；另有 '+bad+' 格資料損壞':'')
      :'找不到已儲存在 10 格欄位的有效進度。';
    $('export').disabled=records.length===0;
    if(bad)$('result').textContent='損壞的欄位無法一鍵備份；請先保留舊 PWA 資料。';
  }catch(error){
    $('status').textContent='讀取舊版存檔失敗：'+(error?.message||error);
    $('export').disabled=true;
  }
}
$('export').addEventListener('click',async()=>{
  $('export').disabled=true;
  $('result').textContent='正在產生本機備份…';
  try{
    const all=await listSaveSlots();
    const valid=all.filter(x=>!x.corrupt);
    if(!valid.length)throw Error('目前沒有可備份的欄位');
    const {blob,count}=createMigrationBundle(valid);
    const url=URL.createObjectURL(blob),link=document.createElement('a');
    link.href=url;link.download='dbz-ssd-all-slots.dbzbackup';
    document.body.append(link);link.click();link.remove();
    setTimeout(()=>URL.revokeObjectURL(url),10000);
    $('result').textContent='已產生 '+count+' 格存檔備份。請到「檔案」確認已保存 dbz-ssd-all-slots.dbzbackup，然後才進行搬遷。';
  }catch(error){
    $('result').textContent='備份未成功：'+(error?.message||error)+'。請勿刪除舊圖示。';
  }finally{$('export').disabled=false;}
});
void scan();
