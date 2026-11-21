// First-phase gameplay-cheat preferences for one browser origin.
// Never persist ROM/IPS, game RAM, save-state bytes or user identifiers.
// Game rules remain owned by the Rust/WASM core; this stores UI choices only.
export const CHEAT_STORAGE_KEY = 'dbz-ssd-cheats-v1';
export const CHEAT_DEFAULTS = Object.freeze({
  damage:1, defence:1, exp:0, z_cards:'off',
  gain:0, encounters:2, max_zeni:false, no_random:false,
});
const BOUNDS = Object.freeze({
  damage:5, defence:5, exp:4, gain:4, encounters:4,
});
export const CHEAT_FIELDS = Object.freeze(Object.keys(CHEAT_DEFAULTS));

export function validateCheatChoices(input){
  if(!input||typeof input!=='object'||Array.isArray(input))return null;
  const clean={};
  for(const key of CHEAT_FIELDS){
    const value=input[key];
    if(key==='max_zeni'||key==='no_random'){
      if(typeof value!=='boolean')return null;
    }else if(key==='z_cards'){
      if(!['off','attack','defence','both'].includes(value))return null;
    }else if(!Number.isInteger(value)||value<0||value>BOUNDS[key]){
      return null;
    }
    clean[key]=value;
  }
  return clean;
}

export function readCheatChoices(storage){
  try{
    if(!storage)return null;
    const raw=storage.getItem(CHEAT_STORAGE_KEY);
    if(!raw)return null;
    const record=JSON.parse(raw);
    if(!record||record.version!==1)return null;
    return validateCheatChoices(record.settings);
  }catch{
    // Safari private browsing, storage disabled and stale/corrupted data all
    // degrade to the built-in neutral defaults, not a broken game.
    return null;
  }
}

export function saveCheatChoices(storage,fromCore){
  const settings=validateCheatChoices(fromCore);
  if(!settings||!storage)return false;
  try{
    storage.setItem(CHEAT_STORAGE_KEY,JSON.stringify({version:1,settings}));
    return true;
  }catch{
    return false;
  }
}

export function restoreCheatChoices(game,storage){
  const settings=readCheatChoices(storage);
  if(!settings)return {found:false,applied:0};
  let applied=0;
  try{
    // Apply before any emulation frames run. Only call the audited first-
    // phase options and avoid writes for neutral settings.
    for(const key of CHEAT_FIELDS){
      if(settings[key]===CHEAT_DEFAULTS[key])continue;
      game.set_cheat_option(key,String(settings[key]));
      applied++;
    }
    return {found:true,applied};
  }catch(error){
    // Never start a partly modified game if one setting cannot be applied.
    try{game.set_cheat_option('reset','all');}catch{}
    return {found:true,applied:0,error:String(error?.message||error)};
  }
}
