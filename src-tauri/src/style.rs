//! The style model: given a photo's features, predict the Lightroom sliders
//! the photographer would have set. Each slider blends two predictors, chosen
//! by cross-validation on the photographer's own edits:
//! - k nearest neighbours ("photos like this one got these settings")
//! - ridge regression (smooth trends, e.g. darker scene -> more exposure)

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::features;

pub struct Slider {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

pub const SLIDERS: &[Slider] = &[
    Slider { key: "Exposure2012", label: "Exposure", min: -5.0, max: 5.0, default: 0.0 },
    Slider { key: "Contrast2012", label: "Contrast", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Highlights2012", label: "Highlights", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Shadows2012", label: "Shadows", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Whites2012", label: "Whites", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Blacks2012", label: "Blacks", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Temperature", label: "Temperature", min: 2000.0, max: 50000.0, default: 5500.0 },
    Slider { key: "Tint", label: "Tint", min: -150.0, max: 150.0, default: 0.0 },
    Slider { key: "Texture", label: "Texture", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Clarity2012", label: "Clarity", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Dehaze", label: "Dehaze", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Vibrance", label: "Vibrance", min: -100.0, max: 100.0, default: 0.0 },
    Slider { key: "Saturation", label: "Saturation", min: -100.0, max: 100.0, default: 0.0 },
];

/// White balance only counts as a learnable target when it was set by hand
/// (or by Imagen); "As Shot" photos don't store a temperature at all.
fn is_wb(key: &str) -> bool {
    key == "Temperature" || key == "Tint"
}

/// Pull a slider's value from parsed develop settings. `None` means "this
/// photo can't teach us anything about this slider".
pub fn target(settings: &Value, s: &Slider) -> Option<f32> {
    if is_wb(s.key) {
        let wb = settings.get("WhiteBalance").and_then(|v| v.as_str()).unwrap_or("As Shot");
        if wb == "As Shot" || wb == "Auto" {
            return None;
        }
    }
    let v = settings.get(s.key).and_then(|v| v.as_f64()).map(|v| v as f32);
    let v = if is_wb(s.key) { v? } else { v.unwrap_or(s.default) };
    let v = undo_preset_amount(settings, s.key, v)?;
    // Temperature is perceptually logarithmic; learn it in mireds.
    Some(if s.key == "Temperature" { 1e6 / v } else { v })
}

/// A preset applied at partial Amount blends its values into the sliders:
/// `stored = base + amount * (preset - base)`. The look isn't part of the
/// per-photo correction we want to learn, so solve for `base`. At full
/// strength the base is gone and the photo can't teach this slider.
fn undo_preset_amount(settings: &Value, key: &str, stored: f32) -> Option<f32> {
    let Some(preset) = settings.get("Preset") else { return Some(stored) };
    let Some(pv) = preset.get("Parameters").and_then(|p| p.get(key)).and_then(|v| v.as_f64()) else {
        return Some(stored);
    };
    let amount = preset.get("Amount").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    if amount > 0.95 {
        return None;
    }
    Some((stored - amount * pv as f32) / (1.0 - amount))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SliderModel {
    pub key: String,
    /// Feature rows (standardized) with a known target, and those targets.
    pub rows: Vec<usize>,
    pub targets: Vec<f32>,
    pub ridge: Vec<f32>, // LEN weights + bias
    pub knn_weight: f32,
    /// Neither predictor beat the photographer's usual value in testing.
    #[serde(default)]
    pub use_mean: bool,
    pub mean: f32,
    pub cv_mae: f32,
    pub baseline_mae: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StyleModel {
    pub version: u32,
    pub name: String,
    pub photo_count: usize,
    pub feat_mean: Vec<f32>,
    pub feat_std: Vec<f32>,
    pub feats: Vec<Vec<f32>>,
    pub sliders: Vec<SliderModel>,
}

const K: usize = 7;
const RIDGE_LAMBDA: f32 = 8.0;

fn standardize(x: &[f32], mean: &[f32], std: &[f32]) -> Vec<f32> {
    x.iter().zip(mean).zip(std).map(|((v, m), s)| (v - m) / s).collect()
}

fn dist2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

fn knn(query: &[f32], feats: &[Vec<f32>], rows: &[usize], targets: &[f32], skip: Option<usize>) -> f32 {
    let mut d: Vec<(f32, f32)> = rows
        .iter()
        .zip(targets)
        .filter(|(r, _)| Some(**r) != skip)
        .map(|(r, t)| (dist2(query, &feats[*r]), *t))
        .collect();
    let k = K.min(d.len());
    if k == 0 {
        return 0.0;
    }
    d.select_nth_unstable_by(k - 1, |a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut num, mut den) = (0.0, 0.0);
    for (dd, t) in &d[..k] {
        let w = 1.0 / (dd.sqrt() + 0.5);
        num += w * t;
        den += w;
    }
    num / den
}

/// Solve (XᵀX + λI) w = Xᵀy with Gaussian elimination. Bias is unpenalized.
fn ridge_fit(xs: &[&[f32]], ys: &[f32]) -> Vec<f32> {
    let d = features::LEN + 1;
    let mut a = vec![vec![0f64; d + 1]; d];
    for (x, y) in xs.iter().zip(ys) {
        let row: Vec<f64> = x.iter().map(|v| *v as f64).chain(std::iter::once(1.0)).collect();
        for i in 0..d {
            for j in 0..d {
                a[i][j] += row[i] * row[j];
            }
            a[i][d] += row[i] * *y as f64;
        }
    }
    for i in 0..d - 1 {
        a[i][i] += RIDGE_LAMBDA as f64;
    }
    a[d - 1][d - 1] += 1e-6;
    for c in 0..d {
        let p = (c..d).max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap()).unwrap();
        a.swap(c, p);
        let piv = a[c][c];
        if piv.abs() < 1e-12 {
            continue;
        }
        for j in c..=d {
            a[c][j] /= piv;
        }
        for i in 0..d {
            if i != c {
                let f = a[i][c];
                if f != 0.0 {
                    for j in c..=d {
                        a[i][j] -= f * a[c][j];
                    }
                }
            }
        }
    }
    a.iter().map(|r| r[d] as f32).collect()
}

fn ridge_predict(w: &[f32], x: &[f32]) -> f32 {
    x.iter().zip(w).map(|(a, b)| a * b).sum::<f32>() + w[features::LEN]
}

pub struct Sample {
    pub features: [f32; features::LEN],
    pub settings: Value,
}

pub fn train(name: &str, samples: &[Sample]) -> StyleModel {
    let n = samples.len().max(1);
    let mut mean = vec![0f32; features::LEN];
    for s in samples {
        for (m, v) in mean.iter_mut().zip(s.features.iter()) {
            *m += v / n as f32;
        }
    }
    let mut std = vec![0f32; features::LEN];
    for s in samples {
        for ((sd, v), m) in std.iter_mut().zip(s.features.iter()).zip(&mean) {
            *sd += (v - m).powi(2) / n as f32;
        }
    }
    let std: Vec<f32> = std.iter().map(|v| v.sqrt().max(1e-3)).collect();
    let feats: Vec<Vec<f32>> = samples.iter().map(|s| standardize(&s.features, &mean, &std)).collect();

    let sliders = SLIDERS
        .iter()
        .map(|sl| {
            let (rows, targets): (Vec<usize>, Vec<f32>) = samples
                .iter()
                .enumerate()
                .filter_map(|(i, s)| target(&s.settings, sl).map(|t| (i, t)))
                .unzip();
            fit_slider(sl, &feats, rows, targets)
        })
        .collect();

    StyleModel {
        version: 1,
        name: name.to_string(),
        photo_count: samples.len(),
        feat_mean: mean,
        feat_std: std,
        feats,
        sliders,
    }
}

fn fit_slider(sl: &Slider, feats: &[Vec<f32>], rows: Vec<usize>, targets: Vec<f32>) -> SliderModel {
    let m = targets.len();
    let mean = if m > 0 { targets.iter().sum::<f32>() / m as f32 } else { 0.0 };
    let baseline_mae = if m > 0 { targets.iter().map(|t| (t - mean).abs()).sum::<f32>() / m as f32 } else { 0.0 };
    if m < 12 {
        return SliderModel {
            key: sl.key.into(),
            rows,
            targets,
            ridge: vec![0.0; features::LEN + 1],
            knn_weight: 0.0,
            use_mean: true,
            mean,
            cv_mae: baseline_mae,
            baseline_mae,
        };
    }

    // 5-fold cross-validation over contiguous blocks. Neighbouring frames of
    // a burst are near-duplicates, so interleaved folds would flatter kNN.
    let folds = 5;
    let fold_of = |i: usize| i * folds / m;
    let mut knn_pred = vec![0f32; m];
    let mut ridge_pred = vec![0f32; m];
    for f in 0..folds {
        let train_idx: Vec<usize> = (0..m).filter(|&i| fold_of(i) != f).collect();
        let xs: Vec<&[f32]> = train_idx.iter().map(|&i| feats[rows[i]].as_slice()).collect();
        let ys: Vec<f32> = train_idx.iter().map(|&i| targets[i]).collect();
        let w = ridge_fit(&xs, &ys);
        let tr_rows: Vec<usize> = train_idx.iter().map(|&i| rows[i]).collect();
        for i in (0..m).filter(|&i| fold_of(i) == f) {
            ridge_pred[i] = ridge_predict(&w, &feats[rows[i]]);
            knn_pred[i] = knn(&feats[rows[i]], feats, &tr_rows, &ys, None);
        }
    }
    let (mut best_w, mut best_mae) = (0.0, f32::MAX);
    for step in 0..=10 {
        let a = step as f32 / 10.0;
        let mae = (0..m)
            .map(|i| (a * knn_pred[i] + (1.0 - a) * ridge_pred[i] - targets[i]).abs())
            .sum::<f32>()
            / m as f32;
        if mae < best_mae {
            best_mae = mae;
            best_w = a;
        }
    }
    // If neither predictor beats "your usual value", use the usual value.
    if best_mae >= baseline_mae {
        return SliderModel {
            key: sl.key.into(),
            rows,
            targets,
            ridge: vec![0.0; features::LEN + 1],
            knn_weight: 0.0,
            use_mean: true,
            mean,
            cv_mae: baseline_mae,
            baseline_mae,
        };
    }
    let xs: Vec<&[f32]> = rows.iter().map(|&r| feats[r].as_slice()).collect();
    let ridge = ridge_fit(&xs, &targets);
    SliderModel { key: sl.key.into(), rows, targets, ridge, knn_weight: best_w, use_mean: false, mean, cv_mae: best_mae, baseline_mae }
}

impl StyleModel {
    /// Predict Lightroom develop settings (real units, clamped to range).
    pub fn predict(&self, raw_features: &[f32; features::LEN]) -> serde_json::Map<String, Value> {
        let x = standardize(raw_features, &self.feat_mean, &self.feat_std);
        let mut out = serde_json::Map::new();
        for (sm, sl) in self.sliders.iter().zip(SLIDERS) {
            if sm.targets.is_empty() {
                continue;
            }
            let v = if sm.use_mean {
                sm.mean
            } else {
                let k = knn(&x, &self.feats, &sm.rows, &sm.targets, None);
                let r = ridge_predict(&sm.ridge, &x);
                sm.knn_weight * k + (1.0 - sm.knn_weight) * r
            };
            let v = if sl.key == "Temperature" { 1e6 / v.max(20.0) } else { v };
            let v = v.clamp(sl.min, sl.max);
            // Lightroom's own precision: exposure to 0.01, the rest whole numbers.
            let v = if sl.key == "Exposure2012" { (v * 100.0).round() / 100.0 } else { v.round() };
            out.insert(sl.key.into(), serde_json::json!(v));
        }
        if out.contains_key("Temperature") {
            out.insert("WhiteBalance".into(), "Custom".into());
        }
        out
    }
}
