//! Tauri commands for the desktop UI.

use base64::Engine as _;
use serde::Serialize;
use serde_json::{Map, Value};
use std::io::Cursor;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

use crate::engine::{self, Config, EditOptions, JobStatus, LightroomState, LookSummary, StyleSummary};
use crate::{catalog, features, presets, raw};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_config() -> Config {
    engine::load_config()
}

#[tauri::command]
pub fn save_config(config: Config) -> Res<()> {
    let style_changed = engine::load_config().style != config.style;
    engine::save_config(&config).map_err(err)?;
    if style_changed && engine::lightroom_state().plugin_installed {
        let _ = engine::write_retouch_presets(config.style.as_deref());
    }
    Ok(())
}

#[tauri::command]
pub fn list_styles() -> Vec<StyleSummary> {
    engine::list_styles()
}

#[tauri::command]
pub fn delete_style(id: String) -> Res<()> {
    engine::delete_style(&id).map_err(err)
}

#[tauri::command]
pub async fn train_style(app: AppHandle, catalog: String, name: String) -> Res<StyleSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        engine::train_style(std::path::Path::new(&catalog), &name, &|p| {
            let _ = app.emit("train-progress", p);
        })
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub fn list_looks() -> Vec<LookSummary> {
    engine::list_looks()
}

#[tauri::command]
pub fn presets_folder() -> Option<String> {
    engine::presets_folder().map(|p| p.to_string_lossy().into())
}

#[tauri::command]
pub fn look_params(path: String) -> Res<Map<String, Value>> {
    engine::look_params(std::path::Path::new(&path)).map_err(err)
}

#[tauri::command]
pub fn lightroom_state() -> LightroomState {
    engine::lightroom_state()
}

#[tauri::command]
pub fn install_plugin() -> Res<String> {
    engine::install_plugin().map(|p| p.to_string_lossy().into()).map_err(err)
}

#[derive(Serialize)]
pub struct RetouchOption {
    id: &'static str,
    label: &'static str,
}

#[tauri::command]
pub fn retouch_options() -> Vec<RetouchOption> {
    presets::RETOUCH.iter().map(|r| RetouchOption { id: r.id, label: r.label }).collect()
}

/// Photos to preview with: the Lightroom selection if there is one,
/// otherwise a spread of photos from the style's training catalog.
#[tauri::command]
pub async fn sample_photos(style: Option<String>, count: usize) -> Res<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let lr = engine::lightroom_state();
        if lr.connected && !lr.selected.is_empty() {
            let step = (lr.selected.len() / count.max(1)).max(1);
            return Ok(lr.selected.into_iter().step_by(step).take(count).collect());
        }
        let id = style.or(engine::load_config().style).ok_or("no style")?;
        let model = engine::load_style(&id).map_err(err)?;
        let mut photos: Vec<_> = catalog::photos(std::path::Path::new(&model.source_catalog))
            .map_err(err)?
            .into_iter()
            .filter(|p| p.exists)
            .collect();
        photos.sort_by(|a, b| a.capture_time.cmp(&b.capture_time));
        let step = (photos.len() / count.max(1)).max(1);
        Ok(photos.iter().step_by(step).take(count).map(|p| p.path.to_string_lossy().into()).collect())
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub struct Preview {
    /// JPEG data URL of the unedited photo.
    url: String,
    width: u32,
    height: u32,
    camera: String,
}

#[tauri::command]
pub async fn load_preview(path: String, max_edge: u32) -> Res<Preview> {
    tauri::async_runtime::spawn_blocking(move || {
        let l = raw::load(&PathBuf::from(&path), max_edge).map_err(err)?;
        let mut buf = Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90).encode_image(&l.image).map_err(err)?;
        Ok(Preview {
            url: format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf.into_inner())),
            width: l.image.width(),
            height: l.image.height(),
            camera: format!("{} {}", l.info.make, l.info.model).trim().to_string(),
        })
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub struct Prediction {
    /// Lightroom slider values the style predicts.
    correction: Map<String, Value>,
    /// White balance relative to as-shot, for the preview renderer.
    temperature_shift: f32,
    tint_shift: f32,
}

#[tauri::command]
pub async fn predict_one(path: String, style: Option<String>) -> Res<Prediction> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = style.or(engine::load_config().style).ok_or("no style")?;
        let model = engine::load_style(&id).map_err(err)?;
        let l = raw::load(&PathBuf::from(&path), 256).map_err(err)?;
        let f = features::extract(&l.image, &l.info);
        let correction = model.predict(&f);
        let (t, n) = model.wb_shift(&f, &correction);
        Ok(Prediction { correction, temperature_shift: t, tint_shift: n })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn edit_selection() -> Res<JobStatus> {
    tauri::async_runtime::spawn_blocking(edit_selection_blocking).await.map_err(err)?
}

/// Edit whatever is selected in Lightroom. Shared by the Edit button and
/// the global shortcut.
pub fn edit_selection_blocking() -> Res<JobStatus> {
    let lr = engine::lightroom_state();
    if !lr.connected {
        return Err("Lightroom isn't connected. Open Lightroom Classic with the Retoucan plugin installed.".into());
    }
    if lr.selected.is_empty() {
        return Err("Select some photos in Lightroom first.".into());
    }
    let paths: Vec<PathBuf> = lr.selected.iter().map(PathBuf::from).collect();
    engine::edit(&paths, &EditOptions::default()).map(|(j, _)| j).map_err(err)
}

#[tauri::command]
pub fn job_status(id: String) -> JobStatus {
    engine::job_status(&id)
}

/// The command an MCP client should run to talk to Retoucan.
#[tauri::command]
pub fn mcp_command() -> Res<String> {
    let exe = std::env::current_exe().map_err(err)?;
    Ok(exe.to_string_lossy().into())
}

#[derive(Serialize)]
pub struct CatalogPhoto {
    path: String,
    name: String,
    folder: String,
    /// 1 = pick, 0 = unflagged, -1 = rejected.
    pick: i8,
    rating: u8,
    label: Option<String>,
    capture_time: Option<String>,
    exists: bool,
    retoucan: bool,
}

#[derive(Serialize)]
pub struct CatalogPhotos {
    catalog: String,
    photos: Vec<CatalogPhoto>,
}

/// Photos in the catalog Lightroom has open (or last had open), with the
/// cull: flags, stars, color labels.
#[tauri::command]
pub async fn catalog_photos(catalog: Option<String>) -> Res<CatalogPhotos> {
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = catalog
            .or(engine::lightroom_state().catalog)
            .ok_or("Open your catalog in Lightroom Classic first.")?;
        let mut photos: Vec<CatalogPhoto> = catalog::photos(std::path::Path::new(&catalog))
            .map_err(err)?
            .into_iter()
            .map(|p| CatalogPhoto {
                name: p.path.file_name().map(|n| n.to_string_lossy().into()).unwrap_or_default(),
                path: p.path.to_string_lossy().into(),
                folder: p.folder,
                pick: p.pick.signum() as i8,
                rating: p.rating.unwrap_or(0.0).clamp(0.0, 5.0) as u8,
                label: p.label,
                capture_time: p.capture_time,
                exists: p.exists,
                retoucan: p.retoucan,
            })
            .collect();
        photos.sort_by(|a, b| a.capture_time.cmp(&b.capture_time).then(a.name.cmp(&b.name)));
        Ok(CatalogPhotos { catalog, photos })
    })
    .await
    .map_err(err)?
}

/// A small JPEG for the photo grid, cached on disk by path and file time.
#[tauri::command]
pub async fn thumbnail(path: String) -> Res<String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::hash::{Hash, Hasher};
        let src = PathBuf::from(&path);
        let modified = std::fs::metadata(&src).and_then(|m| m.modified()).map_err(err)?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&path, modified).hash(&mut h);
        let dir = engine::app_dir().join("thumbs");
        let cached = dir.join(format!("{:016x}.jpg", h.finish()));
        let bytes = match std::fs::read(&cached) {
            Ok(b) => b,
            Err(_) => {
                let l = raw::load(&src, 360).map_err(err)?;
                let mut buf = Cursor::new(Vec::new());
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 80).encode_image(&l.image).map_err(err)?;
                let b = buf.into_inner();
                let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&cached, &b));
                b
            }
        };
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    })
    .await
    .map_err(err)?
}

/// Edit a chosen set of photos (from the Photos screen). With `collection`,
/// Lightroom also gathers them in that collection (the test batch).
#[tauri::command]
pub async fn edit_photos(paths: Vec<String>, collection: Option<String>) -> Res<JobStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        if paths.is_empty() {
            return Err("No photos chosen.".into());
        }
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let opts = EditOptions { collection, ..Default::default() };
        engine::edit(&paths, &opts).map(|(j, _)| j).map_err(err)
    })
    .await
    .map_err(err)?
}

/// "undo" (back to before Retoucan) or "cleanup" (keep edits, drop the undo
/// snapshots) for these photos.
#[tauri::command]
pub fn photo_action(action: String, paths: Vec<String>) -> Res<JobStatus> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    engine::queue_action(&action, &paths).map_err(err)
}
