// HD archive import UI. All reads are local File/OPFS, never fetch/POST player assets.
import {inspectZip,extractZipEntry,verifyOfficialPack,OFFICIAL_PACK} from './hd-pack.mjs';
import {opfsAvailable,saveVerifiedHdPack,readSavedHdPack,clearSavedHdPack} from './hd-storage.mjs';

const $=id=>document.getElementById(id);
let verified=null;
let activeFile=null;
let busy=false;
let persisted=false;
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
function previewEntry(info){return info.entries.find(x=>/\.(png|webp)$/i.test(x.name)&&x.unpacked>=128&&x.unpacked<=2*1024*1024)??null;}
function busyMode(value){
  busy=value;
  $('hd-zip').disabled=value;
  $('hd-verify').disabled=value||!$('hd-zip').files?.[0];
  $('hd-cache').disabled=value||!verified||!opfsAvailable()||persisted;
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
官方原始素材 ZIP 沒有 hdpack.toml／index.bin 是預期情況；後續仍需建立遊戲渲染索引。
驗證只在 iPhone 本機進行。HD 美術套用尚未接入遊戲渲染。 `;
}

$('hd-zip').addEventListener('change',()=>{
  verified=null;activeFile=null;sampleEntry=null;persisted=false;clearPreview();
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
  verified=null;activeFile=null;sampleEntry=null;persisted=false;clearPreview();
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
    sampleEntry=previewEntry(checked);
    status(explanation(checked)+extra+'\n目前只完成驗證，尚未保存。若要下次自動讀取，請按「儲存至本機」。');
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
    const confirmed=await readSavedHdPack();
    if(!confirmed)throw Error('寫入後無法重新讀取 HD 素材包，請保留原始 ZIP');
    persisted=true;
    status(explanation(verified)+'\n本機儲存成功，已重新讀取驗證。重新開啟相同網站會自動辨識；請保留原始 ZIP 作備份。');
    $('hd-progress').value=100;
  }catch(e){
    $('hd-progress').hidden=true;
    let diagnostic='';
    try{
      const estimate=await navigator.storage.estimate?.();
      if(estimate?.quota!=null&&estimate?.usage!=null) {
        const remaining=Math.max(0,estimate.quota-estimate.usage);
        diagnostic='\\nSafari 回報剩餘空間約 '+(remaining/1048576).toFixed(0)+' MiB（僅供估算）。';
      }
    }catch{}
    status('本機分段儲存失敗：'+e.message+diagnostic+'\\n原始 ZIP 仍在「檔案」App，可繼續正常遊玩目前的繁中 Enhanced 2x 版。');
  }
  finally{busyMode(false);}
});
$('hd-remove').addEventListener('click',async()=>{
  if(busy)return;
  busyMode(true);
  try {
    await clearSavedHdPack();
    persisted=false; verified=null; activeFile=null; sampleEntry=null;
    clearPreview();
    status('本機快取已清除。請重新選擇 ZIP；「檔案」App 的原始檔案不受影響。');
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
      const zip=await inspectZip(cached.file);
      activeFile=cached.file;
      verified={...OFFICIAL_PACK,count:zip.count,imageCount:zip.imageCount,manifest:zip.manifest,index:zip.index,entries:zip.entries};
      sampleEntry=previewEntry(verified);
      persisted=true;
      busyMode(false);
      status('已自動恢復上次儲存在 iPhone 的官方 '+OFFICIAL_PACK.version+' HD 素材包（'+(cached.file.size/1048576).toFixed(1)+' MiB，'+zip.count.toLocaleString('en-US')+' 個檔案）。\n可直接預覽 HD 圖片，無須重新選擇 ZIP。上次匯入時已驗證 SHA-256；本次僅核對大小和 ZIP 目錄。\n完整 HD 美術套用尚未完成。');
    } else {
      status('未找到此網站先前保存的 HD 素材包。請匯入並驗證 ZIP；只驗證並不會自動保存。');
    }
  }catch(e){status('讀取舊版 HD 快取失敗：'+e.message);}
})();
