//! Developer CLI for exercising the core without the UI.
//!   retoucan-cli inspect <catalog.lrcat>
//!   retoucan-cli train <catalog.lrcat> <model.json>

use anyhow::{bail, Result};
use rayon::prelude::*;
use retoucan_lib::{catalog, features, raw, style};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("inspect") => inspect(Path::new(&args[2])),
        Some("train") => train(Path::new(&args[2]), Path::new(&args[3])),
        _ => bail!("usage: retoucan-cli inspect|train <catalog.lrcat> [model.json]"),
    }
}

fn inspect(cat: &Path) -> Result<()> {
    let photos = catalog::photos(cat)?;
    let found = photos.iter().filter(|p| p.exists).count();
    println!("{} photos, {} files found on disk", photos.len(), found);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut local: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut tally = |k: String| *counts.entry(k).or_default() += 1;
    for p in &photos {
        let s = &p.settings;
        tally(format!("profile: {}", s.get("CameraProfile").and_then(Value::as_str).unwrap_or("-")));
        tally(format!("look: {}", s.get("Look").and_then(|l| l.get("Name")).and_then(Value::as_str).unwrap_or("-")));
        tally(format!("wb: {}", s.get("WhiteBalance").and_then(Value::as_str).unwrap_or("-")));
        if let Some(Value::Array(groups)) = s.get("MaskGroupBasedCorrections") {
            for g in groups {
                let name = g.get("CorrectionName").and_then(Value::as_str).unwrap_or("?");
                tally(format!("mask: {name}"));
                if let Some(Value::Object(o)) = Some(g) {
                    for (k, v) in o {
                        if let (true, Some(x)) = (k.starts_with("Local"), v.as_f64()) {
                            if x != 0.0 && k != "LocalCurveRefineSaturation" {
                                let e = local.entry(format!("{name} | {k}")).or_insert((0.0, 0));
                                e.0 += x;
                                e.1 += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    for (k, v) in counts {
        println!("{v:5}  {k}");
    }
    println!("\nnon-zero local adjustments (count, mean):");
    for (k, (sum, n)) in local {
        println!("{n:5}  {:>8.3}  {k}", sum / n as f64);
    }
    Ok(())
}

fn train(cat: &Path, out: &Path) -> Result<()> {
    let t = Instant::now();
    let mut photos: Vec<_> = catalog::photos(cat)?.into_iter().filter(|p| p.exists && !p.settings.is_null()).collect();
    photos.sort_by(|a, b| a.capture_time.cmp(&b.capture_time));
    println!("{} usable photos", photos.len());
    let samples: Vec<style::Sample> = photos
        .par_iter()
        .filter_map(|p| match raw::load(&p.path, 256) {
            Ok(l) => Some(style::Sample { features: features::extract(&l.image, &l.info), settings: p.settings.clone() }),
            Err(e) => {
                eprintln!("skip {}: {e}", p.path.display());
                None
            }
        })
        .collect();
    println!("features for {} photos in {:.1}s", samples.len(), t.elapsed().as_secs_f32());
    let model = style::train(cat.file_stem().and_then(|s| s.to_str()).unwrap_or("style"), &samples);
    println!("trained in {:.1}s total\n", t.elapsed().as_secs_f32());
    println!("{:<12} {:>6} {:>10} {:>10} {:>6}", "slider", "n", "guess err", "model err", "knn%");
    for (sm, sl) in model.sliders.iter().zip(style::SLIDERS) {
        let unit = if sl.key == "Temperature" { " mired" } else { "" };
        println!(
            "{:<12} {:>6} {:>10.2} {:>10.2} {:>5.0}%{unit}",
            sl.label, sm.targets.len(), sm.baseline_mae, sm.cv_mae, sm.knn_weight * 100.0
        );
    }
    std::fs::write(out, serde_json::to_vec(&model)?)?;
    println!("\nsaved {}", out.display());
    Ok(())
}
