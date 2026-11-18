// Browser-only Traditional Chinese UI replacement from the live SNES glyphs.
// No OCR, ROM modification or HD art upload. Positions and colours come from the Rust PPU.
const MENU_FALLBACK = Object.freeze({
  'Atk.':'戰鬥','Item':'卡牌','Run':'逃跑','Flee':'逃跑','Menu':'選單',
  'Talk':'交談','Look':'調查','Fly':'舞空術'
});

export function assembleTextRuns(cells) {
  if(!Array.isArray(cells))return [];
  const rows=new Map();
  for(const cell of cells){
    if(!Array.isArray(cell)||cell.length<5)continue;
    const [x,y,ch,ink,paper]=cell;
    if(!Number.isInteger(x)||!Number.isInteger(y)||x<0||x>=256||
       y<0||y>=256||typeof ch!=='string'||ch.length!==1||!Array.isArray(ink)||
       !Array.isArray(paper)||!/^[\x20-\x7e]$/.test(ch))continue;
    if(!rows.has(y))rows.set(y,[]);
    rows.get(y).push({x,y,ch,ink,paper});
  }
  const result=[];
  for(const [y,row] of rows) {
    row.sort((a,b)=>a.x-b.x);
    let group=[];
    const flush=()=>{
      if(!group.length)return;
      const text=group.map(v=>v.ch).join('');
      if(text.trim())result.push({
        x:group[0].x,y,text,
        ink:group[0].ink,paper:group[0].paper,
        width:group[group.length-1].x-group[0].x+8
      });
      group=[];
    };
    for(const cell of row){
      if(group.length && cell.x===group[group.length-1].x)continue;
      if(group.length && cell.x-group[group.length-1].x>9)flush();
      group.push(cell);
    }
    flush();
  }
  return result.sort((a,b)=>a.y-b.y||a.x-b.x);
}

export function reconcileDialogue(message, runs, translationForKey) {
  // The SNES engine can leave battle:88 ("…………") in WRAM while its
  // next message is already visible. Confirm the *actual* rendered English
  // before selecting the translator's existing battle:89 entry.
  const words=new Set(runs.map(r=>r.text.trim().toLowerCase()));
  if(words.has('enemies') && words.has('approach!') &&
    (!message || message.key==='battle:88' || message.stopped)){
    const text=translationForKey('battle:89');
    if(text)return {key:'battle:89',text,progress:1,state:2,stopped:false};
  }
  // The original ellipsis has no semantic content to replace. Do not cover
  // another message with an outdated punctuation-only subtitle.
  if(message?.key==='battle:88')return null;
  return message;
}

const rgb=c=>'rgb('+c.map(n=>Math.max(0,Math.min(255,n|0))).join(',')+')';
export function drawMenuTranslations(ctx, width, height, runs, names={}, dialogueBox=null) {
  if(!runs?.length || !names || width<160 || height<100)return 0;
  const scale=width/256;
  let replaced=0;
  ctx.save();
  ctx.scale(scale,scale);
  ctx.textBaseline='middle';
  for(const run of runs) {
    const english=run.text.trim();
    if(english.length<2||english.length>40)continue;
    let key=english;
    let zh=names[key] || MENU_FALLBACK[key];
    if(!zh) {
      // The game's battle HP table often writes a name and its number in
      // adjacent 8px glyphs: "Piccolo77". Only replace the name's pixels.
      // Require a purely numeric suffix to avoid translating sentence prefixes.
      const prefix=Object.keys(names)
        .filter(name=>name.length>=3 && english.startsWith(name) &&
          /^\d+$/.test(english.slice(name.length)))
        .sort((a,b)=>b.length-a.length)[0];
      if(prefix) {
        key=prefix;
        zh=names[prefix];
      }
    }
    if(!zh || zh===english || /[{\r\n}]/.test(zh))continue;
    // The translated scripted dialogue is rendered separately by its own
    // message box handler. Never replace a random word inside that box.
    if(dialogueBox && run.x<dialogueBox.right && run.x+run.width>dialogueBox.left &&
       run.y>=dialogueBox.top-4 && run.y<=dialogueBox.bottom+3)continue;
    if(run.x<0||run.y<0||run.x+run.width>256||
       !Array.isArray(run.paper)||run.paper.length!==3||
       !Array.isArray(run.ink)||run.ink.length!==3)continue;
    const available=key===english ? run.width : Math.min(run.width,[...key].length*8);
    ctx.font='bold 10px "PingFang TC","Noto Sans TC","Microsoft JhengHei",sans-serif';
    const natural=ctx.measureText(zh).width;
    const size=Math.max(6,Math.min(10,10*available/Math.max(1,natural)));
    ctx.font='bold '+size+'px "PingFang TC","Noto Sans TC","Microsoft JhengHei",sans-serif';
    ctx.fillStyle=rgb(run.paper);
    ctx.fillRect(run.x,run.y-1,available,10);
    ctx.fillStyle=rgb(run.ink);
    ctx.fillText(zh,run.x,run.y+4);
    replaced++;
  }
  ctx.restore();
  return replaced;
}
