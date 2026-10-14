//! Fast image loading. For RAW files we use the camera's embedded JPEG
//! preview instead of demosaicing: it is unedited, full size on most bodies,
//! and decodes in a fraction of the time.

use anyhow::{anyhow, Context, Result};
use image::{imageops::FilterType, DynamicImage, RgbImage};
use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ShotInfo {
    pub make: String,
    pub model: String,
    pub iso: Option<f64>,
    pub exposure_time: Option<f64>,
    pub fnumber: Option<f64>,
    pub focal_length: Option<f64>,
    pub exposure_bias: Option<f64>,
    pub flash_fired: bool,
    pub orientation: u16,
    /// Camera's as-shot white balance multipliers, as ln(R/G) and ln(B/G).
    pub wb_rg: Option<f64>,
    pub wb_bg: Option<f64>,
}

pub struct Loaded {
    pub image: RgbImage,
    pub info: ShotInfo,
}

const RAW_EXTS: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "raf", "orf", "rw2", "pef", "srw", "3fr",
    "iiq", "mrw", "erf", "kdc", "dcr",
];

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RAW_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Load a photo scaled so its long edge is at most `max_edge`, upright.
pub fn load(path: &Path, max_edge: u32) -> Result<Loaded> {
    let (img, info) = if is_raw(path) {
        load_raw_preview(path)?
    } else {
        let img = image::open(path).with_context(|| format!("opening {}", path.display()))?;
        (img, ShotInfo { orientation: 1, ..Default::default() })
    };
    let img = if img.width().max(img.height()) > max_edge {
        // Two-step resize: a cheap box filter gets close, Triangle finishes.
        let mut img = img;
        while img.width().max(img.height()) > max_edge * 3 {
            img = img.resize(img.width() / 2, img.height() / 2, FilterType::Nearest);
        }
        img.resize(max_edge, max_edge, FilterType::Triangle)
    } else {
        img
    };
    let img = orient(img, info.orientation);
    Ok(Loaded { image: img.to_rgb8(), info })
}

fn load_raw_preview(path: &Path) -> Result<(DynamicImage, ShotInfo)> {
    let src = RawSource::new(path).with_context(|| format!("reading {}", path.display()))?;
    let decoder = rawler::get_decoder(&src).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let params = RawDecodeParams::default();
    let img = decoder
        .preview_image(&src, &params)
        .ok()
        .flatten()
        .or_else(|| decoder.full_image(&src, &params).ok().flatten())
        .or_else(|| decoder.thumbnail_image(&src, &params).ok().flatten())
        .ok_or_else(|| anyhow!("{} has no embedded preview", path.display()))?;
    // A dummy decode parses headers only (no pixel data) and is cheap.
    let wb = decoder
        .raw_image(&src, &params, true)
        .ok()
        .map(|r| r.wb_coeffs)
        .filter(|c| c[0] > 0.0 && c[1] > 0.0 && c[2] > 0.0);
    let mut info = decoder
        .raw_metadata(&src, &params)
        .map(|m| {
            let e = &m.exif;
            let rat = |r: Option<rawler::formats::tiff::Rational>| r.filter(|r| r.d != 0).map(|r| r.n as f64 / r.d as f64);
            ShotInfo {
                make: m.make.clone(),
                model: m.model.clone(),
                iso: e
                    .iso_speed_ratings
                    .map(f64::from)
                    .or(e.iso_speed.map(f64::from))
                    .or(e.recommended_exposure_index.map(f64::from)),
                exposure_time: rat(e.exposure_time),
                fnumber: rat(e.fnumber),
                focal_length: rat(e.focal_length),
                exposure_bias: e.exposure_bias.filter(|r| r.d != 0).map(|r| r.n as f64 / r.d as f64),
                flash_fired: e.flash.map(|f| f & 1 == 1).unwrap_or(false),
                orientation: e.orientation.unwrap_or(1),
                ..Default::default()
            }
        })
        .unwrap_or(ShotInfo { orientation: 1, ..Default::default() });
    if let Some(c) = wb {
        info.wb_rg = Some((c[0] as f64 / c[1] as f64).ln());
        info.wb_bg = Some((c[2] as f64 / c[1] as f64).ln());
    }
    Ok((img, info))
}

fn orient(img: DynamicImage, o: u16) -> DynamicImage {
    match o {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}
