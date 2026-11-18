// Browser-only ROM preflight. Actual IPS patching and SHA1 validation happen in ssd-rom.
const CRC_TABLE = new Uint32Array(256);
for (let n = 0; n < 256; ++n) {
  let c = n;
  for (let k = 0; k < 8; ++k) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
  CRC_TABLE[n] = c >>> 0;
}
export function crc32(bytes) {
  let c = 0xFFFFFFFF;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 255] ^ (c >>> 8);
  return (c ^ 0xFFFFFFFF) >>> 0;
}
export const crcHex = n => n.toString(16).toUpperCase().padStart(8, '0');
export function inspectRom(buffer) {
  const bytes = new Uint8Array(buffer);
  const offset = bytes.length % 32768 === 512 ? 512 : 0;
  const validSize = bytes.length > offset && (bytes.length - offset) % 32768 === 0;
  if (!validSize) return { kind: 'invalid', label: 'ROM 大小不正確（可能不是 SNES ROM）' };
  const crc = crcHex(crc32(bytes.subarray(offset)));
  const kind = ({ 'ADE7B968':'jp-rev1', '3BB02A05':'english-unfixed', 'A8CF4F2A':'canonical' })[crc] || 'unknown';
  const labels = {
    'jp-rev1':'已辨識日文 Rev 1，啟動遊戲需要 dbz102.ips',
    'english-unfixed':'已辨識 Klepto v1.02 英文版（未修正校驗碼）',
    canonical:'已辨識可用的 Klepto v1.02 英文版',
    unknown:'非標準版本，可能不相容：請勿強制使用'
  };
  return { kind, crc, offset, label: labels[kind] };
}
