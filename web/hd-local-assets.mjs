// Local-only, on-demand reading of player-provided HD art ZIP.
// No upload, no whole-archive OPFS copy, and no archive-wide decompression.
import { extractZipEntry } from './hd-pack.mjs';

const IMAGE_PATTERN=/\.(?:png|webp)$/i;
export const MAX_SINGLE_ART_BYTES=8*1024*1024;
export const MEMORY_CACHE_LIMIT=16*1024*1024;

export class LocalHdAssets {
  constructor(file, verified, {memoryLimit=MEMORY_CACHE_LIMIT}={}) {
    if(!file?.slice || !file?.size || !verified?.entries?.length)
      throw new Error('請先選擇並驗證 HD 素材 ZIP');
    if(file.size!==verified.bytes)throw new Error('ZIP 大小與已驗證素材包不一致');
    this.file=file;
    this.entries=new Map(verified.entries.map(item=>[item.name,item]));
    this.images=verified.entries.filter(x=>IMAGE_PATTERN.test(x.name));
    this.cache=new Map();
    this.cacheBytes=0;
    this.memoryLimit=memoryLimit;
    this.readCount=0;
  }

  /** Search only directory entries: zero ZIP payload bytes read. */
  search(term='',{limit=20}={}){
    const text=String(term).trim().toLowerCase();
    const n=Math.max(1,Math.min(100,limit));
    return this.images.filter(item=>!text||item.name.toLowerCase().includes(text))
      .slice(0,n).map(({name,unpacked})=>({name,bytes:unpacked}));
  }

  has(name){return this.entries.has(name);}

  async readImage(name){
    const entry=this.entries.get(name);
    if(!entry || !IMAGE_PATTERN.test(entry.name))
      throw new Error('HD 素材不存在：'+name);
    if(entry.unpacked>MAX_SINGLE_ART_BYTES)
      throw new Error('這張 HD 圖片過大，暫時不能載入');
    if(this.cache.has(name)){
      const data=this.cache.get(name);
      // Keep recently used textures at the tail of the LRU.
      this.cache.delete(name);
      this.cache.set(name,data);
      return data;
    }
    const bytes=await extractZipEntry(this.file,entry,{maxBytes:MAX_SINGLE_ART_BYTES});
    const png=entry.name.toLowerCase().endsWith('.png');
    if(png
      ? !(bytes.length>=24&&[137,80,78,71,13,10,26,10].every((value,i)=>bytes[i]===value))
      : !(bytes.length>=16&&String.fromCharCode(...bytes.subarray(0,4))==='RIFF'&&String.fromCharCode(...bytes.subarray(8,12))==='WEBP'))
      throw new Error('素材檔案標頭有誤：'+name);
    this.readCount++;
    if(bytes.byteLength<=this.memoryLimit){
      while(this.cache.size&&this.cacheBytes+bytes.byteLength>this.memoryLimit){
        const [oldKey,oldData]=this.cache.entries().next().value;
        this.cache.delete(oldKey);
        this.cacheBytes-=oldData.byteLength;
      }
      this.cache.set(name,bytes);
      this.cacheBytes+=bytes.byteLength;
    }
    return bytes;
  }
  /** Read official small metadata JSON directly from the ZIP; CRC verified like images. */
  async readJson(name,{maxBytes=2*1024*1024}={}){
    if(!/^(?:bg|cards|class[123]|maps|mode7|variants)\/index\.json$/.test(name))
      throw new Error('只允許讀取官方 HD 素材索引 JSON');
    const entry=this.entries.get(name);
    if(!entry)throw new Error('HD ZIP 找不到索引：'+name);
    const bytes=await extractZipEntry(this.file,entry,{maxBytes});
    let data;
    try{
      data=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
    }catch{throw new Error('HD 素材索引 JSON 無法解析：'+name);}
    if(!data || typeof data!=='object' || Array.isArray(data))
      throw new Error('HD 素材索引格式不正確');
    return data;
  }

  clearCache(){this.cache.clear();this.cacheBytes=0;}
}
