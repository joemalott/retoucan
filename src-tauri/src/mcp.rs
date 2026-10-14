//! MCP server over stdio (`retoucan mcp`), so Claude and other AI tools can
//! drive Retoucan: train styles, pick looks, and edit photos in Lightroom.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::engine::{self, EditOptions, LookChoice};

pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = msg.get("id").cloned() else { continue }; // notification
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let reply = match handle(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32603, "message": e.to_string() } }),
        };
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
    Ok(())
}

fn handle(method: &str, params: &Value) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "retoucan", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Retoucan gives Lightroom Classic photos a first-pass edit in the photographer's own style. \
Typical flow: `status` (is Lightroom connected, what's selected), then `edit_photos` with no paths to edit the current \
Lightroom selection. Edits are applied inside Lightroom by the Retoucan plugin and stay fully editable there. \
Use `list_looks` + `set_defaults` to change the look, and `train_style` to learn a style from an edited catalog."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(name, &args) {
                Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v)? }] }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e.to_string() }], "isError": true }),
            })
        }
        _ => Err(anyhow!("method not found: {method}")),
    }
}

fn tools() -> Value {
    let edit_props = json!({
        "paths": { "type": "array", "items": { "type": "string" },
                   "description": "Photo file paths. Omit to use the photos currently selected in Lightroom." },
        "style": { "type": "string", "description": "Style id (see list_styles). Omit for the default style." },
        "look_uuid": { "type": "string", "description": "Preset UUID from list_looks, or \"none\". Omit for the default look." },
        "look_amount": { "type": "number", "description": "Look strength 0-1 (default: the saved amount, or 1)." },
        "retouch": { "type": "array", "items": { "type": "string", "enum": ["subject", "background", "skin", "teeth", "eyes", "sky"] },
                     "description": "AI-mask retouches to add. Omit for the defaults." }
    });
    json!([
        { "name": "status", "description": "Lightroom connection, current selection, and Retoucan's default style, look and retouches.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "list_styles", "description": "Styles Retoucan has learned, with how accurately each predicts the photographer's sliders.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "train_style", "description": "Learn a new style from an edited Lightroom Classic catalog (.lrcat). Read-only on the catalog; takes about a minute per 1,000 photos.",
          "inputSchema": { "type": "object", "required": ["catalog_path", "name"], "properties": {
              "catalog_path": { "type": "string" }, "name": { "type": "string" } } } },
        { "name": "list_looks", "description": "Develop presets and Look presets installed in Lightroom that can be applied as a look.",
          "inputSchema": { "type": "object", "properties": { "search": { "type": "string", "description": "Filter by name or group" } } } },
        { "name": "set_defaults", "description": "Change the default style, look, look strength, and retouches.",
          "inputSchema": { "type": "object", "properties": {
              "style": { "type": "string" }, "look_uuid": { "type": "string", "description": "UUID or \"none\"" },
              "look_amount": { "type": "number" },
              "retouch": { "type": "array", "items": { "type": "string" } } } } },
        { "name": "predict", "description": "Show the slider values Retoucan would set for photos, without changing anything.",
          "inputSchema": { "type": "object", "properties": edit_props } },
        { "name": "edit_photos", "description": "Edit photos in Lightroom: per-photo exposure/white balance/tone in the chosen style, plus the look and retouches. Applied by the Lightroom plugin within seconds; everything stays editable.",
          "inputSchema": { "type": "object", "properties": edit_props } },
        { "name": "job_status", "description": "Whether Lightroom has applied an edit job yet.",
          "inputSchema": { "type": "object", "required": ["id"], "properties": { "id": { "type": "string" } } } },
        { "name": "install_lightroom_plugin", "description": "Install or update the Retoucan Lightroom plugin and retouch presets. Lightroom must be restarted afterwards.",
          "inputSchema": { "type": "object", "properties": {} } }
    ])
}

fn opt_str<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn find_look(uuid: &str, amount: Option<f64>) -> Result<Option<LookChoice>> {
    if uuid.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let l = engine::list_looks()
        .into_iter()
        .find(|l| l.uuid == uuid)
        .ok_or_else(|| anyhow!("no look with uuid {uuid}; see list_looks"))?;
    Ok(Some(LookChoice { uuid: l.uuid, name: l.name, path: l.path, amount: amount.unwrap_or(1.0).clamp(0.0, 1.0) }))
}

fn edit_options(a: &Value) -> Result<EditOptions> {
    let amount = a.get("look_amount").and_then(Value::as_f64);
    let look = match opt_str(a, "look_uuid") {
        Some(u) => Some(find_look(u, amount)?),
        None => amount.map(|amt| engine::load_config().look.map(|mut l| {
            l.amount = amt.clamp(0.0, 1.0);
            l
        })),
    };
    let retouch = a
        .get("retouch")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(|s| s.as_str().map(String::from)).collect());
    Ok(EditOptions { style: opt_str(a, "style").map(String::from), look, retouch })
}

fn target_paths(a: &Value) -> Result<Vec<PathBuf>> {
    if let Some(p) = a.get("paths").and_then(Value::as_array) {
        if !p.is_empty() {
            return Ok(p.iter().filter_map(|s| s.as_str().map(PathBuf::from)).collect());
        }
    }
    let lr = engine::lightroom_state();
    if !lr.connected {
        return Err(anyhow!(
            "No paths given and Lightroom isn't connected. Open Lightroom Classic (with the Retoucan plugin installed) or pass paths."
        ));
    }
    if lr.selected.is_empty() {
        return Err(anyhow!("Nothing is selected in Lightroom."));
    }
    Ok(lr.selected.into_iter().map(PathBuf::from).collect())
}

fn call(name: &str, a: &Value) -> Result<Value> {
    match name {
        "status" => {
            let lr = engine::lightroom_state();
            let cfg = engine::load_config();
            Ok(json!({
                "lightroom": {
                    "plugin_installed": lr.plugin_installed,
                    "connected": lr.connected,
                    "catalog": lr.catalog,
                    "selected_count": lr.selected.len(),
                    "selected_sample": lr.selected.iter().take(5).collect::<Vec<_>>(),
                },
                "defaults": cfg,
                "styles": engine::list_styles().iter().map(|s| json!({"id": s.id, "name": s.name, "photos": s.photo_count})).collect::<Vec<_>>(),
            }))
        }
        "list_styles" => Ok(serde_json::to_value(engine::list_styles())?),
        "train_style" => {
            let cat = opt_str(a, "catalog_path").ok_or_else(|| anyhow!("catalog_path is required"))?;
            let name = opt_str(a, "name").ok_or_else(|| anyhow!("name is required"))?;
            Ok(serde_json::to_value(engine::train_style(std::path::Path::new(cat), name, &|_| {})?)?)
        }
        "list_looks" => {
            let q = opt_str(a, "search").map(str::to_lowercase);
            let looks: Vec<Value> = engine::list_looks()
                .into_iter()
                .filter(|l| q.as_ref().map(|q| l.name.to_lowercase().contains(q) || l.group.to_lowercase().contains(q)).unwrap_or(true))
                .map(|l| json!({ "uuid": l.uuid, "name": l.name, "group": l.group, "kind": l.kind }))
                .collect();
            Ok(json!({ "count": looks.len(), "looks": looks }))
        }
        "set_defaults" => {
            let mut cfg = engine::load_config();
            if let Some(s) = opt_str(a, "style") {
                engine::load_style(s)?;
                cfg.style = Some(s.into());
            }
            let amount = a.get("look_amount").and_then(Value::as_f64);
            if let Some(u) = opt_str(a, "look_uuid") {
                cfg.look = find_look(u, amount.or(cfg.look.as_ref().map(|l| l.amount)))?;
            } else if let (Some(amt), Some(l)) = (amount, cfg.look.as_mut()) {
                l.amount = amt.clamp(0.0, 1.0);
            }
            if let Some(r) = a.get("retouch").and_then(Value::as_array) {
                cfg.retouch = r.iter().filter_map(|s| s.as_str().map(String::from)).collect();
            }
            engine::save_config(&cfg)?;
            Ok(serde_json::to_value(cfg)?)
        }
        "predict" => {
            let paths = target_paths(a)?;
            let r = engine::resolve(&edit_options(a)?)?;
            let edits = engine::predict(&paths, &r);
            Ok(json!({ "style": r.style_id, "look": r.look.map(|l| l.name), "photos": edits.iter().take(50).map(|e| json!({
                "path": e.path, "correction": e.correction, "error": e.error })).collect::<Vec<_>>() }))
        }
        "edit_photos" => {
            let paths = target_paths(a)?;
            let (job, edits) = engine::edit(&paths, &edit_options(a)?)?;
            // Give a connected Lightroom a moment, so the answer is usually final.
            let mut status = job.clone();
            if engine::lightroom_state().connected {
                for _ in 0..40 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    status = engine::job_status(&job.id);
                    if status.state == "done" {
                        break;
                    }
                }
            }
            let failed: Vec<_> = edits.iter().filter(|e| e.error.is_some()).map(|e| json!({"path": e.path, "error": e.error})).collect();
            Ok(json!({ "job": status, "photos_sent": edits.len() - failed.len(), "unreadable": failed,
                       "note": if status.state == "waiting" { "Queued. Lightroom applies it as soon as it's open with the Retoucan plugin." } else { "" } }))
        }
        "job_status" => Ok(serde_json::to_value(engine::job_status(opt_str(a, "id").unwrap_or("")))?),
        "install_lightroom_plugin" => {
            let dir = engine::install_plugin()?;
            Ok(json!({ "installed_to": dir, "next": "Restart Lightroom Classic to load the plugin and the Retoucan retouch presets." }))
        }
        _ => Err(anyhow!("unknown tool {name}")),
    }
}
