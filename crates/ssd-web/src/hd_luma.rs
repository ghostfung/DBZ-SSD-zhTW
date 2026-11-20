//! Approved class2 greyscale transfer using the desktop engine's
//! alpha-weighted 97% luminance reference and original PPU pixel colour.
//! This is used ONLY for an exact-key pose with confirmed visible OAM ownership.
const KEPT:u64=97;

fn brightness(rgb:&[u8])->u32{
    (u32::from(rgb[0])*299+
     u32::from(rgb[1])*587+
     u32::from(rgb[2])*114)/1000
}
/// Find the largest brightness reference that retains 97% average light.
/// Fully transparent pixels contribute zero weight.
pub(crate) fn reference(rgba:&[u8])->u8{
    let mut histogram=[0_u64;256];
    for p in rgba.chunks_exact(4) {
        histogram[brightness(p) as usize]+=u64::from(p[3]);
    }
    let total: u64=histogram.iter().sum();
    if total==0{return 255;}
    let mut weights=[0_u64;256];
    let mut counts=[0_u64;256];
    let (mut weighted,mut counted)=(0_u64,0_u64);
    for index in 0..256 {
        weighted+=histogram[index]*index as u64;
        counted+=histogram[index];
        weights[index]=weighted;
        counts[index]=counted;
    }
    for r in (1..=255_usize).rev(){
        let keep=weights[r-1]+(total-counts[r-1])*r as u64;
        if 100*keep>=KEPT*total*r as u64{return r as u8;}
    }
    1
}
/// The original SNES pixel supplies the hue. The HD luma image supplies only
/// shading. Do not read the already restored background as the original.
pub(crate) fn colour(original:[u8;3], art:&[u8], reference:u8)->[u8;3]{
    let light=(brightness(art)*255/u32::from(reference.max(1))).min(255);
    original.map(|value|((u32::from(value)*light+127)/255) as u8)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn same_colour_and_shading_rules_as_upstream(){
        let pixels=[100,100,100,255,200,200,200,255];
        let r=reference(&pixels);
        assert!((100..=200).contains(&r));
        let base=[33,180,55];
        let darker=colour(base,&[30,30,30],r);
        let lighter=colour(base,&[220,220,220],r);
        assert!(darker[1]<lighter[1]);
        assert!(lighter[1]<=base[1]);
        assert!(lighter[1]>lighter[0] && lighter[1]>lighter[2]);
        assert_eq!(reference(&[0,0,0,0]),255);
    }
    #[test]
    fn transparency_has_no_influence_on_reference(){
        assert_eq!(reference(&[180,180,180,255,0,0,0,0]),
                   reference(&[180,180,180,255]));
    }
}