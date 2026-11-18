import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash,randomBytes} from 'node:crypto';
import {deflateRawSync} from 'node:zlib';
import {Sha256,sha256File} from '../web/sha256-stream.mjs';
import {OFFICIAL_PACK,inspectZip,extractZipEntry,verifyOfficialPack} from '../web/hd-pack.mjs';

function hash(data){return createHash('sha256').update(data).digest('hex');}
function checksum(buffer){
  let c=0xffffffff;
  for(const byte of buffer){c^=byte;for(let i=0;i<8;i++)c=(c>>>1)^((c&1)?0xedb88320:0);}
  return (c^0xffffffff)>>>0;
}
function sampleZip(paths){
  const local=[],central=[];let offset=0;
  for(const [name,data,method] of paths){
    const fileName=Buffer.from(name);
    const content=Buffer.from(data),packed=method===8?deflateRawSync(content):content;
    const crc=checksum(content);
    const header=Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50,0);header.writeUInt16LE(20,4);header.writeUInt16LE(0x0800,6);
    header.writeUInt16LE(method,8);header.writeUInt32LE(crc,14);
    header.writeUInt32LE(packed.length,18);header.writeUInt32LE(content.length,22);
    header.writeUInt16LE(fileName.length,26);
    local.push(header,fileName,packed);
    const cd=Buffer.alloc(46);
    cd.writeUInt32LE(0x02014b50,0);
    cd.writeUInt16LE(20,4);cd.writeUInt16LE(20,6);cd.writeUInt16LE(0x0800,8);
    cd.writeUInt16LE(method,10);cd.writeUInt32LE(crc,16);
    cd.writeUInt32LE(packed.length,20);cd.writeUInt32LE(content.length,24);
    cd.writeUInt16LE(fileName.length,28);cd.writeUInt32LE(offset,42);
    central.push(cd,fileName);
    offset+=header.length+fileName.length+packed.length;
  }
  const centralData=Buffer.concat(central),eocd=Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50,0);
  eocd.writeUInt16LE(paths.length,8);eocd.writeUInt16LE(paths.length,10);
  eocd.writeUInt32LE(centralData.length,12);eocd.writeUInt32LE(offset,16);
  return new Blob([...local,centralData,eocd]);
}

test('SHA-256 known vectors and chunk boundaries',()=>{
  for(const text of ['','abc','hello world','a'.repeat(101),'b'.repeat(65537)]){
    const bytes=Buffer.from(text);
    const instance=new Sha256();
    for(let i=0;i<bytes.length;i+=7)instance.update(bytes.subarray(i,i+7));
    assert.equal(instance.digestHex(),hash(bytes),text.length);
  }
});
test('SHA-256 produces expected digest for random buffers and file chunk IO',async()=>{
  const bytes=randomBytes(123_457);
  const sha=new Sha256();
  for(let i=0;i<bytes.length;i+=1031)sha.update(bytes.subarray(i,i+1031));
  assert.equal(sha.digestHex(),hash(bytes));
  assert.equal(await sha256File(new Blob([bytes]),{chunkSize:2047}),hash(bytes));
});
test('ZIP central directory inspection and safe CRC-protected extraction',async()=>{
  const file=sampleZip([
    ['hdpack/hdpack.toml',Buffer.from('format = 1\nscale = 4\n'),8],
    ['hdpack/index.bin',Buffer.from('SSDHDIDX'),0],
    ['hdpack/art/example.png',Buffer.from('png'),8]
  ]);
  const zip=await inspectZip(file);
  assert.equal(zip.count,3);
  assert.equal(zip.imageCount,1);
  assert.equal(zip.manifest,'hdpack/hdpack.toml');
  assert.equal(zip.index,'hdpack/index.bin');
  const manifest=zip.entries.find(x=>x.name===zip.manifest);
  if(typeof DecompressionStream==='function'){
    const text=await extractZipEntry(file,manifest);
    assert.equal(new TextDecoder().decode(text),'format = 1\nscale = 4\n');
  }
  const index=zip.entries.find(x=>x.name===zip.index);
  assert.equal(new TextDecoder().decode(await extractZipEntry(file,index)),'SSDHDIDX');
});
test('ZIP rejects unsafe paths, wrong structure and unrelated HD pack',async()=>{
  await assert.rejects(inspectZip(new Blob(['bad'])),/ZIP/);
  await assert.rejects(inspectZip(sampleZip([['../bad.png','unsafe',0]])),/不安全/);
  await assert.rejects(verifyOfficialPack(sampleZip([['hdpack.toml','fake',0]])),/大小不符/);
  assert.equal(OFFICIAL_PACK.bytes,484121119);
  assert.equal(OFFICIAL_PACK.sha256,'e55cb05d4cb98904d4893b3597a90b7259893c34797f3b8e5b5c1cd0b621d4cc');
});
