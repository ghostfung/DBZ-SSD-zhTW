//! Safely matched field maps in the iPhone browser.
//! Identity comes from the upstream WRAM layout and a PPU BG2 tile audit,
//! never from map appearance, world coordinates alone or a label.
use serde::Deserialize;
use ssd_hd::{mapart, world::{check_tiles, FieldState, Rom}};
use ssd_oracle::FrameLog;
use std::collections::{HashMap,HashSet};

const SCREEN:usize=256;
const GAME_TILE:usize=256;
const IMAGE_SIDE:usize=512;
const MAX_LAYERS:usize=256;
const MAX_TILES:usize=8;
const IMAGE_BYTES:usize=IMAGE_SIDE*IMAGE_SIDE*4;

#[derive(Deserialize)]
struct Input {
    id:String, maps:Vec<u8>, blocks:[u8;2], grid:String,
    metatiles:String, #[serde(default)] source:String,
    #[serde(default)] palette:String,
    #[serde(default)] palette_variant:bool,
    tiles:String,
}
#[derive(Clone)]
struct Active {
    index:usize, origin:(i32,i32), size:(i32,i32),
    wanted:Vec<(usize,usize)>,
}
#[derive(Default)]
pub(crate) struct LiveMaps {
    index:mapart::Index,
    active:Option<Active>,
    pictures:HashMap<(usize,usize,usize),Vec<u8>>,
    reason:&'static str,
    checked_tiles:u64,
    mismatched_tiles:u64,
}

fn safe_template(path:&str)->bool {
    path.starts_with("maps/") && path.len()<=220 &&
    !path.contains("..") && !path.contains('\\') &&
    path.contains("{x}") && path.contains("{y}") &&
    (path.ends_with(".png") || path.ends_with(".webp"))
}
fn signed(value:i32,modulus:i32)->i32 {
    let rest=value.rem_euclid(modulus);
    if rest>=modulus/2 {rest-modulus} else {rest}
}
fn origin(log:&FrameLog,camera:(u16,u16))->Option<(i32,i32)> {
    let mut found=None;
    for entry in &log.entries {
        if entry.from_pixel != 0 {return None;}
        let regs=&entry.regs;
        if regs.forced_blanking {continue;}
        if regs.bg_mode!=1 || !(regs.main_bg[1] || regs.sub_bg[1])
            || regs.bg_large_tiles[1] || regs.brightness!=15 ||
            regs.bg_color_math[1] && regs.color_math!=0
        {return None;}
        let (camx,camy)=(i32::from(camera.0),i32::from(camera.1));
        let point=(camx+signed(i32::from(regs.bg_h_scroll[1])-camx,512),
            camy+signed(i32::from(regs.bg_v_scroll[1])-camy,256));
        match found {
            Some(old) if old!=point=>return None,
            None=>found=Some(point),
            _=>{}
        }
    }
    found
}
impl LiveMaps {
    pub(crate) fn has_entries(&self)->bool{!self.index.layer.is_empty()}
    pub(crate) fn has_pending_art(&self)->bool {
        self.active.as_ref().is_some_and(|a|
            a.wanted.iter().any(|&(x,y)|
                !self.pictures.contains_key(&(a.index,x,y))))
    }
    pub(crate) fn load_catalog(&mut self,json:&str)->Result<usize,String>{
        let input:Vec<Input>=serde_json::from_str(json)
            .map_err(|e|format!("地圖 HD 索引格式錯誤：{e}"))?;
        if input.len()>MAX_LAYERS {return Err("地圖索引超過安全上限".into());}
        let mut layer=Vec::new();
        for item in input {
            if item.id.is_empty() || item.id.len()>128 ||
                !safe_template(&item.tiles) ||
                item.blocks.iter().any(|&n| !(1..=12).contains(&n)) ||
                mapart::parse_pointer(&item.grid).is_none() ||
                mapart::parse_pointer(&item.metatiles).is_none()
            {return Err("地圖 HD 索引存在不安全的路徑或佈局".into());}
            layer.push(mapart::Entry{
                id:item.id,maps:item.maps,blocks:item.blocks,
                grid:item.grid,metatiles:item.metatiles,
                source:item.source,palette:item.palette,
                palette_variant:item.palette_variant,tiles:item.tiles
            });
        }
        self.index=mapart::Index{layer};
        self.active=None;self.pictures.clear();self.reason="尚未匹配";
        Ok(self.index.layer.len())
    }
    pub(crate) fn refresh(&mut self,rom:&[u8],state:Option<FieldState>,log:&FrameLog){
        self.active=None;self.checked_tiles=0;self.mismatched_tiles=0;
        self.reason="地圖 PPU 尚未匹配";
        if self.index.layer.is_empty(){self.reason="無 maps/index.json 素材";return;}
        if !log.complete {self.reason="PPU 畫面不完整";return;}
        if !log.cgram_writes.is_empty(){self.reason="場景調色盤正在變化";return;}
        let Some(state)=state else {self.reason="沒有 WRAM 地圖狀態";return;};
        let state=state.in_room(Rom(rom));
        if !(ssd_hd::world::is_field_mode(state.game_mode) ||
            state.game_mode&0x7f==1 && state.map_id>=48) {
            self.reason="目前不是可驗證的探索地圖";return;
        }
        let Some(from)=origin(log,state.camera) else {
            self.reason="BG2 視窗／捲動未穩定";return;
        };
        let (grid,metatiles)=state.map.layout();
        let Some(index)=self.index.find(state.map.blocks,grid,metatiles,state.map_id)
            else {self.reason="沒有與 ROM 地圖指標吻合的 HD 佈局";return;};
        // Reused ROM layouts can have distinct palette art. Without an
        // explicit map ID, never assume another map's HD colours are safe.
        let entry=&self.index.layer[index];
        if !entry.maps.is_empty() && !entry.maps.contains(&state.map_id) {
            self.reason="同一地圖佈局被其他地圖使用，拒絕錯誤配色";return;
        }
        let check=check_tiles(Rom(rom),&state,log);
        self.checked_tiles=check.tiles;
        self.mismatched_tiles=check.differing;
        let inside=check.tiles.saturating_sub(check.outside);
        if check.tiles<24 || inside<20 || !check.is_map() ||
            check.differing.saturating_mul(20)>inside
        {self.reason="VRAM BG2 與 ROM 地圖圖塊不吻合";return;}
        let (width,height)=state.map.size();
        let mut wanted=HashSet::new();
        for y in [from.1+1,from.1+i32::from(log.height)] {
            for x in [from.0,from.0+255] {
                if x>=0&&y>=0&&x<width&&y<height {
                    wanted.insert((x as usize/GAME_TILE,y as usize/GAME_TILE));
                }
            }
        }
        // If the window spans a tile boundary, the four corners include
        // each displayed 256px map art tile.
        if wanted.len()>MAX_TILES {self.reason="地圖視窗超出安全上限";return;}
        let mut wanted=wanted.into_iter().collect::<Vec<_>>();
        wanted.sort_unstable();
        self.active=Some(Active{index,origin:from,size:(width,height),wanted});
        self.reason="地圖佈局與 PPU BG2 校驗通過";
    }
    pub(crate) fn matches_json(&self)->String {
        let Some(active)=&self.active else{return "[]".into()};
        let entry=&self.index.layer[active.index];
        serde_json::Value::Array(active.wanted.iter().map(|&(x,y)|{
            serde_json::json!({"index":active.index,"tileX":x,"tileY":y,
                "id":entry.id,"art":entry.tile_file(x,y),
                "ready":self.pictures.contains_key(&(active.index,x,y))})
        }).collect()).to_string()
    }
    pub(crate) fn probe_json(&self)->String{
        serde_json::json!({"reason":self.reason,"checked":self.checked_tiles,
            "different":self.mismatched_tiles,
            "matched":self.active.as_ref().map_or(0,|a|a.wanted.len())}).to_string()
    }
    pub(crate) fn add_picture(&mut self,index:usize,x:usize,y:usize,
        width:usize,height:usize,rgba:&[u8])->Result<(),String>{
        let active=self.active.as_ref().ok_or("地圖場景已切換")?;
        if index!=active.index || !active.wanted.contains(&(x,y)) {
            return Err("地圖場景已切換，拒絕過期地圖磚".into());
        }
        if width!=IMAGE_SIDE || height!=IMAGE_SIDE || rgba.len()!=IMAGE_BYTES {
            return Err("HD 地圖磚尺寸必須為 512×512（2x）".into());
        }
        self.pictures.insert((index,x,y),rgba.to_vec());
        if self.pictures.len()>MAX_TILES {
            self.pictures.retain(|&(i,tx,ty),_|i==index &&
                active.wanted.contains(&(tx,ty)));
        }
        Ok(())
    }
    pub(crate) fn composite(&self,log:&FrameLog,layers:&[u8],
        rgba:&mut[u8],width:usize,height:usize)->u32{
        let Some(active)=&self.active else{return 0};
        if width!=512 || height!=usize::from(log.height)*2 ||
            layers.len()!=SCREEN*usize::from(log.height) ||
            rgba.len()!=width*height*4{return 0;}
        let mut changed=0;
        for y in 0..usize::from(log.height) {
            let wy=active.origin.1+y as i32+1;
            if wy<0 || wy>=active.size.1 {continue;}
            let ty=wy as usize/GAME_TILE;
            for x in 0..SCREEN {
                let p=y*SCREEN+x;
                if layers[p]!=2 {continue;}
                let wx=active.origin.0+x as i32;
                if wx<0 || wx>=active.size.0 {continue;}
                let tx=wx as usize/GAME_TILE;
                let Some(img)=self.pictures.get(&(active.index,tx,ty)) else {continue};
                let (localx,localy)=(wx as usize%GAME_TILE*2,wy as usize%GAME_TILE*2);
                let mut different=false;
                for sy in 0..2 {
                    for sx in 0..2 {
                        let src=((localy+sy)*IMAGE_SIDE+localx+sx)*4;
                        let dst=((y*2+sy)*width+x*2+sx)*4;
                        let alpha=u32::from(img[src+3]);
                        if alpha==0{continue;}
                        for ch in 0..3 {
                            let old=u32::from(rgba[dst+ch]);
                            let next=((u32::from(img[src+ch])*alpha+
                                old*(255-alpha)+127)/255) as u8;
                            different|=next!=rgba[dst+ch];
                            rgba[dst+ch]=next;
                        }
                        rgba[dst+3]=255;
                    }
                }
                changed+=u32::from(different);
            }
        }
        changed
    }
}
