//! Low overhead per-stage HD profiler. Sample 1 in 8 emulation frames.
//! Browser timer is imported only on wasm; native tests use synthetic data.
//! No ROM, IPS, ZIP paths or picture pixels enter diagnostic output.
use serde_json::json;

pub(crate) const EMU:usize=0;
pub(crate) const PPU:usize=1;
pub(crate) const BG:usize=2;
pub(crate) const MAP:usize=3;
pub(crate) const MODE7:usize=4;
pub(crate) const FIGHTER:usize=5;
pub(crate) const PORTRAIT:usize=6;
pub(crate) const CARDS:usize=7;
pub(crate) const TEXT:usize=8;
const COUNT:usize=9;

#[cfg(target_arch="wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js=
    "export function ssd_profile_now() { return performance.now(); }")]
extern "C" {
    fn ssd_profile_now()->f64;
}
#[inline]
pub(crate) fn now()->f64 {
    #[cfg(target_arch="wasm32")]
    {ssd_profile_now()}
    #[cfg(not(target_arch="wasm32"))]
    {0.0}
}
#[derive(Default)]
pub(crate) struct Profile {
    sum:[f64;COUNT],
    max:[f64;COUNT],
    samples:u32,
}
impl Profile {
    pub(crate) fn mark(&mut self,stage:usize,active:bool,last:&mut f64){
        if !active||stage>=COUNT{return;}
        let current=now();
        let elapsed=(current-*last).max(0.0);
        *last=current;
        if elapsed.is_finite()&&elapsed<2000.0{
            self.sum[stage]+=elapsed;
            self.max[stage]=self.max[stage].max(elapsed);
        }
    }
    pub(crate) fn finish(&mut self,active:bool){
        if active{self.samples=self.samples.saturating_add(1);}
    }
    pub(crate) fn take_json(&mut self)->String{
        const NAMES:[&str;COUNT]=[
            "emulation","ppu","background","map","mode7",
            "fighters","portrait","cards","translation"
        ];
        let n=self.samples;
        let rows=NAMES.iter().enumerate().map(|(i,name)|
            (name.to_string(),json!({
                "avg":if n>0 {self.sum[i]/f64::from(n)}else{0.0},
                "max":self.max[i]
            }))).collect::<serde_json::Map<String,serde_json::Value>>();
        let result=json!({"samples":n,"stages":rows}).to_string();
        *self=Self::default();
        result
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn phase_samples_do_not_accumulate_after_read(){
        let mut profile=Profile::default();
        profile.sum[PPU]=15.0;
        profile.max[PPU]=10.0;
        profile.finish(true);
        profile.finish(true);
        let data:serde_json::Value=serde_json::from_str(&profile.take_json()).unwrap();
        assert_eq!(data["samples"],2);
        assert_eq!(data["stages"]["ppu"]["avg"],7.5);
        assert_eq!(data["stages"]["ppu"]["max"],10.0);
        assert_eq!(profile.samples,0);
        assert_eq!(profile.sum[PPU],0.0);
    }
}
