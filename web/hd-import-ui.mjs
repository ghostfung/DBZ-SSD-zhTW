// HD archive import UI. All reads are local File/OPFS, never fetch/POST player assets.
import {inspectZip,extractZipEntry,verifyOfficialPack,OFFICIAL_PACK} from './hd-pack.mjs';
import {opfsAvailable,saveVerifiedHdPack,readSavedHdPack,clearSavedHdPack} from './hd-storage.mjs';

const $=id=>document.getElementById(id);
let verified=null;
let activeFile=null;
let busy=false;
let previewObjectUrl=null;
let sampleEntry=null;

function clearPreview(){
  if(previewObjectUrl)URL.revokeObjectURL(previewObjectUrl);
  previewObjectUrl=null;
  $('hd-preview-panel').hidden=true;
  $('hd-preview-image').removeAttribute('src');
  $('hd-preview-name').textContent='';
}

function status(message){$('hd-status').textContent=message;}
function busyMode(value){
  busy=value;
  $('hd-zip').disabled=value;
  $('hd-verify').disabled=value||!$('hd-zip').files?.[0];
  $('hd-cache').disabled=value||!verified||!opfsAvailable();
  $('hd-preview-btn').disabled=value||!sampleEntry||!verified;
  $('hd-remove').disabled=value;
}
function progress(phase,done,total){
  const value=total?Math.round(done/total*100):0;
  $('hd-progress').hidden=false;
  $('hd-progress').value=value;
  status(`${phase}：${value}%（${(done/1048576).toFixed(0)} / ${(total/1048576).toFixed(0)} MiB）`);
}
function explanation(result){
  return `官方 ${result.version} HD 美術包已通過 SHA-256 驗證。
${result.bytes.toLocaleString('en-US')} bytes，ZIP 內有 ${result.count.toLocaleString('en-US')} 個項目、${result.imageCount.toLocaleString('en-US')} 張 PNG/WebP。
${result.manifest?'有 hdpack.toml：'+result.manifest:'未見 hdpack.toml'}；${result.index?'有 index.bin：'+result.index:'未見 index.bin'}。
驗證只在 iPhone 本機進行。HD 美術套用尚未接入遊戲渲染。 `;
}

$('hd-zip').addEventListener('change',()=>{
  verified=null;activeFile=null;sampleEntry=null;clearPreview();
  $('hd-preview-btn').disabled=true;
  $('hd-progress').hidden=true;
  const file=$('hd-zip').files?.[0];
  $('hd-verify').disabled=!file;
  $('hd-cache').disabled=true;
  status(file?`已選擇 ${file.name}（${(file.size/1048576).toFixed(1)} MiB），請按「驗證 HD 美術包」。`:'尚未選擇 HD 美術包。');
});
$('hd-verify').addEventListener('click',async()=>{
  const file=$('hd-zip').files?.[0];
  if(!file||busy)return;
  busyMode(true);
  verified=null;activeFile=null;sampleEntry=null;clearPreview();
  try {
    status('正在檢查 ZIP 結構...');
    // Import is deliberately strict: only the original signed GitHub asset digest is accepted.
    const checked=await verifyOfficialPack(file,{onProgress:(n,total)=>progress('正在驗證 SHA-256',n,total)});
    let extra='';
    if(checked.manifest){
      const index=await inspectZip(file);
      const entry=index.entries.find(x=>x.name===checked.manifest);
      if(entry?.unpacked<=256*1024){
        try {
          const raw=await extractZipEntry(file,entry,{maxBytes:256*1024});
          const text=new TextDecoder().decode(raw);
          const match=text.match(/^\s*format\s*=\s*(\d+)/m);
          extra=match?`\n素材索引 format=${match[1]}`:'';
        }catch(e){extra=`\n索引文字檢查未完成：${e.message}`;}
      }
    }
    verified=checked;activeFile=file;
    sampleEntry=checked.entries.find(x=>/\.(png|webp)$/i.test(x.name) && x.unpacked>=128 && x.unpacked<=2*1024*1024)??null;
    status(explanation(checked)+extra);
    $('hd-progress').value=100;
  } catch(e){
    $('hd-progress').hidden=true;
    status('HD 素材包未通過驗證：'+e.message);
  } finally {busyMode(false);}
});

$('hd-preview-btn').addEventListener('click',async()=>{
  if(!activeFile||!sampleEntry||!verified||busy)return;
  busyMode(true);
  try {
    clearPreview();
    const entry=sampleEntry;
    const data=await extractZipEntry(activeFile,entry,{maxBytes:2*1024*1024});
    const isPng=entry.name.toLowerCase().endsWith('.png');
    const valid=isPng
      ? data.length>24&&data[0]===137&&data[1]===80&&data[2]===78&&data[3]===71
      : data.length>12&&String.fromCharCode(...data.subarray(0,4))==='RIFF'&&
        String.fromCharCode(...data.subarray(8,12))==='WEBP';
    if(!valid)throw Error('圖片檔案標頭不符合 PNG/WebP');
    previewObjectUrl=URL.createObjectURL(new Blob([data],{type:isPng?'image/png':'image/webp'}));
    $('hd-preview-image').src=previewObjectUrl;
    $('hd-preview-name').textContent='已從手機本機 ZIP 解壓並校驗：'+entry.name+'（此為素材預覽，尚未套用到遊戲）';
    $('hd-preview-panel').hidden=false;
  }catch(e){status('HD 圖片測試失敗：'+e.message);}
  finally{busyMode(false);}
});
$('hd-cache').addEventListener('click',async()=>{
  if(!verified||!activeFile||busy)return;
  busyMode(true);
  try {
    await saveVerifiedHdPack(activeFile,verified,{onProgress:(n,total)=>progress('正在存到 iPhone 本機 OPFS',n,total)});
    status(explanation(verified)+'\n已儲存到此網站的本機私有空間，重新開啟可讀取。清除網站資料可能會刪除此備份。');
    $('hd-progress').value=100;
  }catch(e){$('hd-progress').hidden=true;status('本機儲存失敗：'+e.message+'。原始 ZIP 仍留在「檔案」App。');}
  finally{busyMode(false);}
});
$('hd-remove').addEventListener('click',async()=>{
  if(busy)return;
  busyMode(true);
  try {
    await clearSavedHdPack();
    status('本機快取已清除；你在「檔案」App 的原始 ZIP 不受影響。');
    $('hd-progress').hidden=true;
  }catch(e){status('無法清除本機快取：'+e.message);}
  finally{busyMode(false);}
});

void (async()=>{
  if(!opfsAvailable()){
    status('目前瀏覽器沒有 OPFS，仍可從「檔案」App 選擇 ZIP，完成本機驗證。');
    return;
  }
  try {
    const cached=await readSavedHdPack();
    if(cached) {
      status(`已找到先前驗證並存於此網站的官方 ${OFFICIAL_PACK.version} HD 美術包（${(cached.file.size/1048576).toFixed(1)} MiB）。素材套用功能尚未完成。`);
    }
  }catch(e){status('讀取舊版 HD 快取失敗：'+e.message);}
})();
