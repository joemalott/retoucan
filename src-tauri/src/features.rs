//! Describe a photo as a fixed-length vector of numbers the style model can
//! compare: tone distribution, color cast, skin tones, framing, and the
//! camera settings it was shot with. Computed from a small (~256px) preview.

use image::RgbImage;

use crate::raw::ShotInfo;

pub const LEN: usize = 46;

pub const NAMES: [&str; LEN] = [
    "lum_p01", "lum_p05", "lum_p25", "lum_p50", "lum_p75", "lum_p95", "lum_p99", "lum_mean", "lum_std",
    "clip_hi", "clip_lo", "hist_0", "hist_1", "hist_2", "hist_3", "hist_4", "hist_5", "hist_6", "hist_7",
    "rg_all", "bg_all", "rg_mid", "bg_mid", "rg_hi", "bg_hi", "sat_mean", "sat_p90", "skin_frac",
    "skin_lum", "skin_rg", "skin_bg", "center_lum", "edge_lum", "top_lum", "top_bg", "local_contrast",
    "log_iso", "log_shutter", "fnumber", "flash", "exp_bias", "aspect", "ev100", "focal",
    "wb_rg", "wb_bg",
];

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn extract(img: &RgbImage, info: &ShotInfo) -> [f32; LEN] {
    let lut: Vec<f32> = (0..=255u8).map(srgb_to_linear).collect();
    let (w, h) = img.dimensions();
    let n = (w * h).max(1) as f32;

    let mut lums = Vec::with_capacity((w * h) as usize);
    let mut hist = [0f32; 8];
    let (mut sr, mut sg, mut sb) = (0f64, 0f64, 0f64);
    let (mut mr, mut mg, mut mb, mut mn) = (0f64, 0f64, 0f64, 0f64);
    let (mut hr, mut hg, mut hb, mut hn) = (0f64, 0f64, 0f64, 0f64);
    let (mut kr, mut kg, mut kb, mut kl, mut kn) = (0f64, 0f64, 0f64, 0f64, 0f64);
    let mut sats = Vec::with_capacity((w * h) as usize);
    let (mut clip_hi, mut clip_lo) = (0f32, 0f32);
    let (mut center, mut center_n, mut edge, mut edge_n) = (0f64, 0f64, 0f64, 0f64);
    let (mut top, mut top_b, mut top_g, mut top_n) = (0f64, 0f64, 0f64, 0f64);

    for (x, y, p) in img.enumerate_pixels() {
        let [r8, g8, b8] = p.0;
        let (r, g, b) = (lut[r8 as usize], lut[g8 as usize], lut[b8 as usize]);
        // Perceptual lightness proxy: luma of the gamma-encoded values.
        let l = (0.2126 * r8 as f32 + 0.7152 * g8 as f32 + 0.0722 * b8 as f32) / 255.0;
        lums.push(l);
        hist[((l * 8.0) as usize).min(7)] += 1.0;
        sr += r as f64;
        sg += g as f64;
        sb += b as f64;
        if (0.25..0.75).contains(&l) {
            mr += r as f64;
            mg += g as f64;
            mb += b as f64;
            mn += 1.0;
        }
        if l >= 0.75 && r8.max(g8).max(b8) < 250 {
            hr += r as f64;
            hg += g as f64;
            hb += b as f64;
            hn += 1.0;
        }
        if r8.max(g8).max(b8) >= 250 {
            clip_hi += 1.0;
        }
        if r8.max(g8).max(b8) <= 5 {
            clip_lo += 1.0;
        }
        let mx = r8.max(g8).max(b8) as f32;
        let mnv = r8.min(g8).min(b8) as f32;
        sats.push(if mx > 0.0 { (mx - mnv) / mx } else { 0.0 });

        // Classic YCbCr skin box; loose on purpose, lighting varies a lot.
        let yv = 0.299 * r8 as f32 + 0.587 * g8 as f32 + 0.114 * b8 as f32;
        let cb = 128.0 - 0.168736 * r8 as f32 - 0.331264 * g8 as f32 + 0.5 * b8 as f32;
        let cr = 128.0 + 0.5 * r8 as f32 - 0.418688 * g8 as f32 - 0.081312 * b8 as f32;
        if yv > 40.0 && (77.0..=127.0).contains(&cb) && (133.0..=173.0).contains(&cr) {
            kr += r as f64;
            kg += g as f64;
            kb += b as f64;
            kl += l as f64;
            kn += 1.0;
        }

        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        if (0.25..0.75).contains(&fx) && (0.2..0.8).contains(&fy) {
            center += l as f64;
            center_n += 1.0;
        } else if !(0.1..0.9).contains(&fx) || !(0.1..0.9).contains(&fy) {
            edge += l as f64;
            edge_n += 1.0;
        }
        if fy < 0.2 {
            top += l as f64;
            top_b += b as f64;
            top_g += g as f64;
            top_n += 1.0;
        }
    }

    // Local contrast: mean absolute difference to the right/down neighbour.
    let mut lc = 0f64;
    for y in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            lc += (lums[i] - lums[i + 1]).abs() as f64 + (lums[i] - lums[i + w as usize]).abs() as f64;
        }
    }
    lc /= (2 * w.saturating_sub(1).max(1) * h.saturating_sub(1).max(1)) as f64;

    let lum_mean = lums.iter().sum::<f32>() / n;
    let lum_std = (lums.iter().map(|l| (l - lum_mean).powi(2)).sum::<f32>() / n).sqrt();
    let mut sorted = lums.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f32| sorted[((sorted.len() - 1) as f32 * p) as usize];
    sats.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let sat_mean = sats.iter().sum::<f32>() / n;
    let sat_p90 = sats[((sats.len() - 1) as f32 * 0.9) as usize];

    // Log channel ratios: 0 means neutral. Fall back to the global ratio
    // when a region is empty so the feature still means something.
    let ratio = |a: f64, b: f64| ((a + 1e-4) / (b + 1e-4)).ln() as f32;
    let rg_all = ratio(sr, sg);
    let bg_all = ratio(sb, sg);
    let (rg_mid, bg_mid) = if mn > 10.0 { (ratio(mr, mg), ratio(mb, mg)) } else { (rg_all, bg_all) };
    let (rg_hi, bg_hi) = if hn > 10.0 { (ratio(hr, hg), ratio(hb, hg)) } else { (rg_mid, bg_mid) };
    let skin_frac = (kn / n as f64) as f32;
    let (skin_lum, skin_rg, skin_bg) = if kn > 10.0 {
        ((kl / kn) as f32, ratio(kr, kg), ratio(kb, kg))
    } else {
        (lum_mean, rg_mid, bg_mid)
    };
    let avg = |s: f64, c: f64| if c > 0.0 { (s / c) as f32 } else { lum_mean };

    let log_iso = info.iso.map(|v| (v / 100.0).log2() as f32).unwrap_or(1.0);
    let log_shutter = info.exposure_time.map(|v| v.log2() as f32).unwrap_or(-7.0);
    let fnum = info.fnumber.unwrap_or(4.0) as f32;
    // Scene brightness from the camera's settings (EV at ISO 100). Tells a
    // dim reception apart from a dark-toned photo of a bright scene.
    let ev100 = match (info.fnumber, info.exposure_time, info.iso) {
        (Some(f), Some(t), Some(iso)) if t > 0.0 && iso > 0.0 => ((f * f / t).log2() - (iso / 100.0).log2()) as f32,
        _ => 8.0,
    };

    [
        pct(0.01), pct(0.05), pct(0.25), pct(0.50), pct(0.75), pct(0.95), pct(0.99), lum_mean, lum_std,
        clip_hi / n, clip_lo / n,
        hist[0] / n, hist[1] / n, hist[2] / n, hist[3] / n, hist[4] / n, hist[5] / n, hist[6] / n, hist[7] / n,
        rg_all, bg_all, rg_mid, bg_mid, rg_hi, bg_hi, sat_mean, sat_p90, skin_frac, skin_lum, skin_rg, skin_bg,
        avg(center, center_n), avg(edge, edge_n), avg(top, top_n), if top_n > 0.0 { ratio(top_b, top_g) } else { bg_all },
        lc as f32,
        log_iso, log_shutter, fnum, if info.flash_fired { 1.0 } else { 0.0 },
        info.exposure_bias.unwrap_or(0.0) as f32, w as f32 / h.max(1) as f32, ev100,
        info.focal_length.map(|f| (f as f32).ln()).unwrap_or(3.9),
        info.wb_rg.unwrap_or(0.7) as f32, info.wb_bg.unwrap_or(0.4) as f32,
    ]
}
