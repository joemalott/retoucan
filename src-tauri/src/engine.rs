//! Everything Retoucan can do, in one place. The desktop UI, the command
//! line, and the MCP server are thin wrappers around these functions.
//!
//! Lightroom stays the only writer of its catalog: Retoucan drops edit jobs
//! into a bridge folder and the Retoucan Lightroom plugin applies them.

use anyhow::{anyhow, bail, Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{catalog, features, presets, raw, style};

pub const APP_ID: &str = "com.joemalott.retoucan";

pub fn app_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(APP_ID)
}
fn styles_dir() -> PathBuf {
    app_dir().join("styles")
}
pub fn bridge_dir() -> PathBuf {
    app_dir().join("bridge")
}
fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

/// Lightroom Classic loads every plugin in this folder at startup.
pub fn lightroom_modules_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        dirs::data_dir().map(|d| d.join("Adobe").join("Lightroom").join("Modules"))
    } else {
        dirs::home_dir().map(|d| d.join("Library/Application Support/Adobe/Lightroom/Modules"))
    }
}

// ---------------------------------------------------------------------------
// Settings

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LookChoice {
    pub uuid: String,
    pub name: String,
    pub path: PathBuf,
    /// 0..1, like Lightroom's preset Amount slider.
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub style: Option<String>,
    pub look: Option<LookChoice>,
    pub retouch: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { style: None, look: None, retouch: vec!["subject".into(), "background".into(), "skin".into(), "teeth".into()] }
    }
}

pub fn load_config() -> Config {
    std::fs::read(config_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_config(c: &Config) -> Result<()> {
    std::fs::create_dir_all(app_dir())?;
    std::fs::write(config_path(), serde_json::to_vec_pretty(c)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Styles

#[derive(Debug, Clone, Serialize)]
pub struct StyleSummary {
    pub id: String,
    pub name: String,
    pub photo_count: usize,
    pub source_catalog: String,
    pub created: String,
    /// Per slider: (label, typical miss of a plain average, typical miss of the model).
    pub accuracy: Vec<(String, f32, f32)>,
    pub retouch_learned: Vec<String>,
}

fn summarize(id: &str, m: &style::StyleModel) -> StyleSummary {
    StyleSummary {
        id: id.into(),
        name: m.name.clone(),
        photo_count: m.photo_count,
        source_catalog: m.source_catalog.clone(),
        created: m.created.clone(),
        accuracy: m
            .sliders
            .iter()
            .zip(style::SLIDERS)
            .filter(|(s, _)| !s.targets.is_empty())
            .map(|(s, sl)| (sl.label.to_string(), s.baseline_mae, s.cv_mae))
            .collect(),
        retouch_learned: m.retouch.keys().cloned().collect(),
    }
}

pub fn list_styles() -> Vec<StyleSummary> {
    let mut out: Vec<StyleSummary> = std::fs::read_dir(styles_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let id = p.file_stem()?.to_str()?.to_string();
            let m: style::StyleModel = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            Some(summarize(&id, &m))
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn load_style(id: &str) -> Result<style::StyleModel> {
    let p = styles_dir().join(format!("{}.json", slug(id)));
    let bytes = std::fs::read(&p).with_context(|| format!("style \"{id}\" not found"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn delete_style(id: &str) -> Result<()> {
    std::fs::remove_file(styles_dir().join(format!("{}.json", slug(id))))?;
    Ok(())
}

fn slug(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
}

/// Learn a style from an edited Lightroom catalog. Reads only: the catalog
/// is opened read-only and photos are read from their embedded previews.
pub fn train_style(catalog_path: &Path, name: &str, progress: &(dyn Fn(Progress) + Sync)) -> Result<StyleSummary> {
    let mut photos: Vec<_> = catalog::photos(catalog_path)?
        .into_iter()
        .filter(|p| p.exists && p.settings.get("ProcessVersion").is_some())
        .collect();
    if photos.len() < 20 {
        bail!(
            "Found {} edited photos with their original files. Retoucan needs at least 20 (a few hundred is better). \
             If the photos live on an external drive, plug it in first.",
            photos.len()
        );
    }
    photos.sort_by(|a, b| a.capture_time.cmp(&b.capture_time));
    let total = photos.len();
    let done = AtomicUsize::new(0);
    progress(Progress { stage: "Looking at your photos".into(), done: 0, total });
    let samples: Vec<style::Sample> = photos
        .par_iter()
        .filter_map(|p| {
            let r = raw::load(&p.path, 256).ok().map(|l| style::Sample {
                features: features::extract(&l.image, &l.info),
                settings: p.settings.clone(),
            });
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 10 == 0 || n == total {
                progress(Progress { stage: "Looking at your photos".into(), done: n, total });
            }
            r
        })
        .collect();
    progress(Progress { stage: "Learning your style".into(), done: total, total });
    let mut model = style::train(name, &samples);
    model.source_catalog = catalog_path.to_string_lossy().into();
    model.created = now_iso();
    std::fs::create_dir_all(styles_dir())?;
    let id = slug(name);
    if id.is_empty() {
        bail!("Give the style a name");
    }
    std::fs::write(styles_dir().join(format!("{id}.json")), serde_json::to_vec(&model)?)?;
    let mut cfg = load_config();
    if cfg.style.is_none() {
        cfg.style = Some(id.clone());
        save_config(&cfg)?;
    }
    Ok(summarize(&id, &model))
}

// ---------------------------------------------------------------------------
// Looks

#[derive(Debug, Clone, Serialize)]
pub struct LookSummary {
    pub uuid: String,
    pub name: String,
    pub group: String,
    pub kind: String,
    pub path: PathBuf,
    pub has_table: bool,
}

pub fn list_looks() -> Vec<LookSummary> {
    presets::list()
        .into_iter()
        .filter(|p| p.group != "Retoucan")
        .map(|p| LookSummary {
            uuid: p.uuid,
            name: p.name.trim().to_string(),
            group: p.group.trim().to_string(),
            kind: p.kind,
            path: p.path,
            has_table: p.has_table,
        })
        .collect()
}

pub fn look_params(path: &Path) -> Result<Map<String, Value>> {
    Ok(presets::read(path)?.params)
}

// ---------------------------------------------------------------------------
// Editing

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EditOptions {
    pub style: Option<String>,
    /// None = use the saved choice; Some(None) = no look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Option<LookChoice>>,
    pub retouch: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhotoEdit {
    pub path: PathBuf,
    /// Final develop settings for Lightroom (style + look blended).
    pub settings: Map<String, Value>,
    /// Style-only prediction, for previews and explanations.
    pub correction: Map<String, Value>,
    pub error: Option<String>,
}

pub struct Resolved {
    pub model: style::StyleModel,
    pub style_id: String,
    pub look: Option<LookChoice>,
    pub look_params: Option<Map<String, Value>>,
    pub retouch: Vec<String>,
}

pub fn resolve(opts: &EditOptions) -> Result<Resolved> {
    let cfg = load_config();
    let style_id = opts
        .style
        .clone()
        .or(cfg.style.clone())
        .or_else(|| list_styles().first().map(|s| s.id.clone()))
        .ok_or_else(|| anyhow!("No style yet. Train one from an edited Lightroom catalog first."))?;
    let model = load_style(&style_id)?;
    let look = match &opts.look {
        Some(l) => l.clone(),
        None => cfg.look.clone(),
    };
    let look_params = match &look {
        Some(l) => Some(look_params(&l.path).with_context(|| format!("reading look {}", l.name))?),
        None => None,
    };
    let retouch = opts.retouch.clone().unwrap_or(cfg.retouch.clone());
    Ok(Resolved { model, style_id, look, look_params, retouch })
}

pub fn predict(paths: &[PathBuf], r: &Resolved) -> Vec<PhotoEdit> {
    paths
        .par_iter()
        .map(|p| match raw::load(p, 256) {
            Ok(l) => {
                let correction = r.model.predict(&features::extract(&l.image, &l.info));
                let settings = match (&r.look, &r.look_params) {
                    (Some(look), Some(params)) => presets::blend(&correction, params, look.amount),
                    _ => correction.clone(),
                };
                PhotoEdit { path: p.clone(), settings, correction, error: None }
            }
            Err(e) => PhotoEdit { path: p.clone(), settings: Map::new(), correction: Map::new(), error: Some(e.to_string()) },
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub id: String,
    pub state: String, // "waiting" | "done" | "missing"
    pub applied: Option<usize>,
    pub skipped: Option<usize>,
    pub message: Option<String>,
}

/// Queue edits for Lightroom. The plugin's watcher applies them within a
/// second or two while Lightroom is open; otherwise they wait.
pub fn queue_job(edits: &[PhotoEdit], r: &Resolved) -> Result<JobStatus> {
    let jobs = bridge_dir().join("jobs");
    std::fs::create_dir_all(&jobs)?;
    let id = format!("{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis());
    let mut lua = String::from("return {\n  version = 1,\n");
    lua += &format!("  id = {},\n", lua_str(&id));
    if let Some(look) = &r.look {
        lua += &format!(
            "  look = {{ uuid = {}, name = {}, amount = {} }},\n",
            lua_str(&look.uuid),
            lua_str(&look.name),
            look.amount
        );
    }
    lua += "  retouch = {";
    for id in &r.retouch {
        if presets::RETOUCH.iter().any(|x| x.id == id) {
            lua += &format!(" {},", lua_str(&presets::retouch_uuid(id)));
        }
    }
    lua += " },\n  photos = {\n";
    for e in edits.iter().filter(|e| e.error.is_none()) {
        lua += &format!("    {{ path = {}, settings = {} }},\n", lua_str(&e.path.to_string_lossy()), lua_table(&e.settings));
    }
    lua += "  },\n}\n";
    // Write then rename, so the plugin never reads a half-written job.
    let tmp = jobs.join(format!("{id}.tmp"));
    std::fs::write(&tmp, lua)?;
    std::fs::rename(&tmp, jobs.join(format!("{id}.lua")))?;
    Ok(JobStatus { id, state: "waiting".into(), applied: None, skipped: None, message: None })
}

pub fn job_status(id: &str) -> JobStatus {
    let done = bridge_dir().join("done").join(format!("{id}.json"));
    if let Some(v) = std::fs::read(&done).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
        return JobStatus {
            id: id.into(),
            state: "done".into(),
            applied: v.get("applied").and_then(Value::as_u64).map(|n| n as usize),
            skipped: v.get("skipped").and_then(Value::as_u64).map(|n| n as usize),
            message: v.get("message").and_then(Value::as_str).map(String::from),
        };
    }
    let waiting = bridge_dir().join("jobs").join(format!("{id}.lua")).exists();
    JobStatus { id: id.into(), state: if waiting { "waiting" } else { "missing" }.into(), applied: None, skipped: None, message: None }
}

/// Edit photos end to end: predict, then queue for Lightroom.
pub fn edit(paths: &[PathBuf], opts: &EditOptions) -> Result<(JobStatus, Vec<PhotoEdit>)> {
    let r = resolve(opts)?;
    let edits = predict(paths, &r);
    if edits.iter().all(|e| e.error.is_some()) {
        bail!("Couldn't read any of those photos: {}", edits.first().and_then(|e| e.error.clone()).unwrap_or_default());
    }
    Ok((queue_job(&edits, &r)?, edits))
}

// ---------------------------------------------------------------------------
// Lightroom link

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LightroomState {
    pub connected: bool,
    pub catalog: Option<String>,
    pub selected: Vec<String>,
    pub seconds_ago: Option<u64>,
    pub plugin_installed: bool,
}

/// What the plugin last reported. It refreshes every couple of seconds while
/// Lightroom is open, so a stale heartbeat means Lightroom is closed.
pub fn lightroom_state() -> LightroomState {
    let installed = lightroom_modules_dir().map(|d| d.join("Retoucan.lrplugin/Info.lua").exists()).unwrap_or(false);
    let p = bridge_dir().join("state.json");
    let Some(v) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
        return LightroomState { plugin_installed: installed, ..Default::default() };
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let ago = v.get("time").and_then(Value::as_f64).map(|t| now.saturating_sub(t as u64));
    LightroomState {
        connected: ago.map(|a| a < 15).unwrap_or(false),
        catalog: v.get("catalog").and_then(Value::as_str).map(String::from),
        selected: v
            .get("selected")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        seconds_ago: ago,
        plugin_installed: installed,
    }
}

const PLUGIN_FILES: &[(&str, &str)] = &[
    ("Info.lua", include_str!("../../lightroom/Retoucan.lrplugin/Info.lua")),
    ("Bridge.lua", include_str!("../../lightroom/Retoucan.lrplugin/Bridge.lua")),
    ("Init.lua", include_str!("../../lightroom/Retoucan.lrplugin/Init.lua")),
    ("Shutdown.lua", include_str!("../../lightroom/Retoucan.lrplugin/Shutdown.lua")),
    ("EditSelected.lua", include_str!("../../lightroom/Retoucan.lrplugin/EditSelected.lua")),
    ("ApplyNow.lua", include_str!("../../lightroom/Retoucan.lrplugin/ApplyNow.lua")),
];

const STARTER_LOOKS: &[(&str, &str)] = &[
    ("golden-film.xmp", include_str!("../../looks/golden-film.xmp")),
    ("soft-pastel.xmp", include_str!("../../looks/soft-pastel.xmp")),
    ("warm-negative.xmp", include_str!("../../looks/warm-negative.xmp")),
    ("clean-bright.xmp", include_str!("../../looks/clean-bright.xmp")),
    ("moody-matte.xmp", include_str!("../../looks/moody-matte.xmp")),
    ("teal-and-amber.xmp", include_str!("../../looks/teal-and-amber.xmp")),
    ("faded-vintage.xmp", include_str!("../../looks/faded-vintage.xmp")),
    ("classic-bandw.xmp", include_str!("../../looks/classic-bandw.xmp")),
];

/// Install (or update) the Lightroom plugin, the retouch presets, and
/// Retoucan's starter looks.
/// Lightroom picks both up the next time it starts.
pub fn install_plugin() -> Result<PathBuf> {
    let dir = lightroom_modules_dir().ok_or_else(|| anyhow!("can't find Lightroom's plugin folder"))?.join("Retoucan.lrplugin");
    std::fs::create_dir_all(&dir)?;
    for (name, body) in PLUGIN_FILES {
        std::fs::write(dir.join(name), body)?;
    }
    let exe = std::env::current_exe()?;
    std::fs::create_dir_all(bridge_dir().join("jobs"))?;
    std::fs::create_dir_all(bridge_dir().join("done"))?;
    std::fs::write(
        dir.join("Config.lua"),
        format!(
            "return {{ bridge = {}, app = {} }}\n",
            lua_str(&bridge_dir().to_string_lossy()),
            lua_str(&exe.to_string_lossy())
        ),
    )?;
    write_retouch_presets(None)?;
    let looks = presets::settings_dir().ok_or_else(|| anyhow!("no presets folder"))?.join("Retoucan Looks");
    std::fs::create_dir_all(&looks)?;
    for (name, body) in STARTER_LOOKS {
        std::fs::write(looks.join(name), body)?;
    }
    Ok(dir)
}

/// Retouch presets use the active style's learned values when it has them.
pub fn write_retouch_presets(style_id: Option<&str>) -> Result<PathBuf> {
    let id = style_id.map(String::from).or(load_config().style);
    let learned = id.and_then(|i| load_style(&i).ok()).map(|m| m.retouch).unwrap_or_default();
    presets::write_retouch_presets(&learned)
}

// ---------------------------------------------------------------------------
// Helpers

fn lua_str(s: &str) -> String {
    let mut level = 0;
    while s.contains(&format!("]{}]", "=".repeat(level))) {
        level += 1;
    }
    let eq = "=".repeat(level);
    format!("[{eq}[{s}]{eq}]")
}

fn lua_value(v: &Value) -> String {
    match v {
        Value::Null => "nil".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => lua_str(s),
        Value::Array(a) => format!("{{ {} }}", a.iter().map(lua_value).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => lua_table(o),
    }
}

fn lua_table(m: &Map<String, Value>) -> String {
    let body: Vec<String> = m
        .iter()
        .map(|(k, v)| {
            if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()) {
                format!("{k} = {}", lua_value(v))
            } else {
                format!("[{}] = {}", lua_str(k), lua_value(v))
            }
        })
        .collect();
    format!("{{ {} }}", body.join(", "))
}

fn now_iso() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    // Civil date from days since epoch (Howard Hinnant's algorithm).
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}
