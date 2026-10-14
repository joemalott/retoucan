//! Lightroom / Camera Raw develop presets (.xmp): find and read the user's
//! presets, and write Retoucan's own retouch presets with AI masks.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where Lightroom Classic and Camera Raw look for user presets.
pub fn settings_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        dirs::data_dir().map(|d| d.join("Adobe").join("CameraRaw").join("Settings"))
    } else {
        dirs::home_dir().map(|d| d.join("Library/Application Support/Adobe/CameraRaw/Settings"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    pub uuid: String,
    pub name: String,
    pub group: String,
    /// "Normal" for develop presets, "Look" for profile-like looks.
    pub kind: String,
    pub path: PathBuf,
    /// Has an embedded color table (a LUT); the preview can't show that part.
    pub has_table: bool,
    pub params: Map<String, Value>,
}

/// Every preset under Lightroom's settings folder, skipping section
/// dividers (names made of underscores) and unreadable files.
pub fn list() -> Vec<PresetInfo> {
    let Some(root) = settings_dir() else { return vec![] };
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&root).max_depth(3).into_iter().flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("xmp")) != Some(true) {
            continue;
        }
        if let Ok(info) = read(p) {
            if info.name.trim().trim_matches('_').is_empty() || info.uuid.is_empty() {
                continue;
            }
            out.push(info);
        }
    }
    out.sort_by(|a, b| (&a.group, a.name.trim()).cmp(&(&b.group, b.name.trim())));
    out
}

pub fn read(path: &Path) -> Result<PresetInfo> {
    let text = std::fs::read_to_string(path)?;
    let mut params = Map::new();
    let mut has_table = false;

    // Simple attributes on the main rdf:Description: crs:Key="value".
    let head_end = text.find("<crs:Name>").unwrap_or(text.len());
    let mut rest = &text[..head_end];
    while let Some(i) = rest.find("crs:") {
        rest = &rest[i + 4..];
        let Some(eq) = rest.find("=\"") else { break };
        let key = &rest[..eq];
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        let raw = &after[..end];
        rest = &after[end..];
        if key.starts_with("Table_") || key == "RGBTable" {
            has_table = true;
            continue;
        }
        let v = match raw.trim_start_matches('+').parse::<f64>() {
            Ok(f) => serde_json::json!(f),
            Err(_) => match raw {
                "True" | "true" => Value::Bool(true),
                "False" | "false" => Value::Bool(false),
                _ => Value::String(raw.to_string()),
            },
        };
        params.insert(key.to_string(), v);
    }
    if text.contains("crs:Table_") || text.contains("crs:RGBTable") {
        has_table = true;
    }

    // Tone curves: <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li>...
    for key in ["ToneCurvePV2012", "ToneCurvePV2012Red", "ToneCurvePV2012Green", "ToneCurvePV2012Blue"] {
        if let Some(body) = element(&text, key) {
            let pts: Vec<Value> = body
                .split("<rdf:li>")
                .skip(1)
                .filter_map(|li| li.split("</rdf:li>").next())
                .flat_map(|pair| pair.split(',').map(|n| n.trim().parse::<f64>().ok()).collect::<Vec<_>>())
                .flatten()
                .map(|f| serde_json::json!(f))
                .collect();
            if !pts.is_empty() {
                params.insert(key.into(), Value::Array(pts));
            }
        }
    }

    let alt = |key: &str| {
        element(&text, key)
            .and_then(|b| b.split('>').nth(2).map(|s| s.split('<').next().unwrap_or("").to_string()))
            .unwrap_or_default()
    };
    let uuid = params.get("UUID").and_then(Value::as_str).unwrap_or("").to_string();
    let kind = params.get("PresetType").and_then(Value::as_str).unwrap_or("Normal").to_string();
    let mut group = alt("Group");
    if group.is_empty() {
        group = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("").replace('_', " ");
    }
    Ok(PresetInfo {
        uuid,
        name: alt("Name"),
        group,
        kind,
        path: path.to_path_buf(),
        has_table,
        params,
    })
}

fn element<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let open = format!("<crs:{key}>");
    let close = format!("</crs:{key}>");
    let s = text.find(&open)? + open.len();
    let e = text[s..].find(&close)? + s;
    Some(&text[s..e])
}

/// Develop settings Lightroom treats as "amount-able" when blending a preset
/// at partial strength, with their neutral values. Hues and absolute white
/// balance are not blended.
pub const BLENDABLE: &[(&str, f64)] = &[
    ("Exposure2012", 0.0), ("Contrast2012", 0.0), ("Highlights2012", 0.0), ("Shadows2012", 0.0),
    ("Whites2012", 0.0), ("Blacks2012", 0.0), ("Texture", 0.0), ("Clarity2012", 0.0), ("Dehaze", 0.0),
    ("Vibrance", 0.0), ("Saturation", 0.0), ("ParametricShadows", 0.0), ("ParametricDarks", 0.0),
    ("ParametricLights", 0.0), ("ParametricHighlights", 0.0),
    ("HueAdjustmentRed", 0.0), ("HueAdjustmentOrange", 0.0), ("HueAdjustmentYellow", 0.0),
    ("HueAdjustmentGreen", 0.0), ("HueAdjustmentAqua", 0.0), ("HueAdjustmentBlue", 0.0),
    ("HueAdjustmentPurple", 0.0), ("HueAdjustmentMagenta", 0.0),
    ("SaturationAdjustmentRed", 0.0), ("SaturationAdjustmentOrange", 0.0), ("SaturationAdjustmentYellow", 0.0),
    ("SaturationAdjustmentGreen", 0.0), ("SaturationAdjustmentAqua", 0.0), ("SaturationAdjustmentBlue", 0.0),
    ("SaturationAdjustmentPurple", 0.0), ("SaturationAdjustmentMagenta", 0.0),
    ("LuminanceAdjustmentRed", 0.0), ("LuminanceAdjustmentOrange", 0.0), ("LuminanceAdjustmentYellow", 0.0),
    ("LuminanceAdjustmentGreen", 0.0), ("LuminanceAdjustmentAqua", 0.0), ("LuminanceAdjustmentBlue", 0.0),
    ("LuminanceAdjustmentPurple", 0.0), ("LuminanceAdjustmentMagenta", 0.0),
    ("SplitToningShadowSaturation", 0.0), ("SplitToningHighlightSaturation", 0.0),
    ("ColorGradeMidtoneSat", 0.0), ("ColorGradeGlobalSat", 0.0), ("ColorGradeShadowLum", 0.0),
    ("ColorGradeMidtoneLum", 0.0), ("ColorGradeHighlightLum", 0.0), ("ColorGradeGlobalLum", 0.0),
    ("GrainAmount", 0.0), ("PostCropVignetteAmount", 0.0),
];

/// `base + amount * (preset - base)` for every blendable key the preset sets.
/// Keys the base doesn't have start from their neutral value.
pub fn blend(base: &Map<String, Value>, preset: &Map<String, Value>, amount: f64) -> Map<String, Value> {
    let mut out = base.clone();
    for (key, neutral) in BLENDABLE {
        let Some(p) = preset.get(*key).and_then(Value::as_f64) else { continue };
        let b = base.get(*key).and_then(Value::as_f64).unwrap_or(*neutral);
        let v = b + amount * (p - b);
        let v = if *key == "Exposure2012" { (v * 100.0).round() / 100.0 } else { v.round() };
        out.insert((*key).into(), serde_json::json!(v));
    }
    out
}

// ---------------------------------------------------------------------------
// Retouch presets (AI masks)

#[derive(Debug, Clone)]
pub struct Retouch {
    pub id: &'static str,
    pub label: &'static str,
    /// (MaskSubType, MaskSubCategoryID, MaskName). Subtype 1 = subject,
    /// 2 = sky, 3 = a person part, 0 + category 22 = background.
    pub masks: &'static [(u8, Option<u8>, &'static str)],
    /// Default local adjustments, in Lightroom's stored units (-1..1).
    pub defaults: &'static [(&'static str, f64)],
    /// Substrings of Imagen/Lightroom correction names to learn values from.
    pub learn_from: &'static [&'static str],
}

pub const RETOUCH: &[Retouch] = &[
    Retouch {
        id: "subject",
        label: "Brighten subject",
        masks: &[(1, None, "Subject")],
        defaults: &[("LocalExposure2012", 0.026), ("LocalShadows2012", 0.035), ("LocalWhites2012", 0.17)],
        learn_from: &["Subjects Mask", "Subject"],
    },
    Retouch {
        id: "background",
        label: "Darken background",
        masks: &[(0, Some(22), "Background")],
        defaults: &[("LocalExposure2012", -0.026), ("LocalHighlights2012", -0.035), ("LocalBlacks2012", -0.17)],
        learn_from: &["Background"],
    },
    Retouch {
        id: "skin",
        label: "Smooth skin",
        masks: &[(3, Some(2), "Face Skin")],
        defaults: &[("LocalTexture", -0.35), ("LocalClarity2012", -0.15)],
        learn_from: &["Smooth Skin", "Skin"],
    },
    Retouch {
        id: "teeth",
        label: "Whiten teeth",
        masks: &[(3, Some(12), "Teeth")],
        defaults: &[
            ("LocalExposure2012", 0.05),
            ("LocalHighlights2012", 0.3),
            ("LocalSaturation", -0.1),
            ("LocalTemperature", -0.1),
        ],
        learn_from: &["Teeth"],
    },
    Retouch {
        id: "eyes",
        label: "Brighten eyes",
        masks: &[(3, Some(8), "Eye Sclera"), (3, Some(3), "Iris and Pupil")],
        defaults: &[
            ("LocalSaturation", 0.073),
            ("LocalSharpness", 0.144),
            ("LocalHighlights2012", 0.2),
            ("LocalShadows2012", 0.2),
            ("LocalWhites2012", 0.2),
            ("LocalClarity2012", 0.21),
        ],
        learn_from: &["Eyes", "Iris"],
    },
    Retouch {
        id: "sky",
        label: "Deepen sky",
        masks: &[(2, None, "Sky")],
        defaults: &[("LocalExposure2012", -0.05), ("LocalHighlights2012", -0.2), ("LocalSaturation", 0.1)],
        learn_from: &["Sky"],
    },
];

/// Average the local adjustments of every mask group whose name matches a
/// retouch's `learn_from`, so Retoucan reuses the photographer's (or
/// Imagen's) actual numbers instead of our defaults.
pub fn learn_retouch(settings: &[&Value]) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut sums: BTreeMap<String, (BTreeMap<String, f64>, usize)> = BTreeMap::new();
    for s in settings {
        let Some(Value::Array(groups)) = s.get("MaskGroupBasedCorrections") else { continue };
        for g in groups {
            let name = g.get("CorrectionName").and_then(Value::as_str).unwrap_or("");
            let Some(r) = RETOUCH.iter().find(|r| r.learn_from.iter().any(|l| name.contains(l))) else { continue };
            let Some(obj) = g.as_object() else { continue };
            let e = sums.entry(r.id.into()).or_default();
            e.1 += 1;
            for (k, v) in obj {
                if let (true, Some(x)) = (k.starts_with("Local"), v.as_f64()) {
                    if k != "LocalCurveRefineSaturation" {
                        *e.0.entry(k.clone()).or_default() += x;
                    }
                }
            }
        }
    }
    sums.into_iter()
        .filter(|(_, (_, n))| *n >= 5)
        .map(|(id, (m, n))| {
            let avg = m
                .into_iter()
                .map(|(k, v)| (k, ((v / n as f64) * 1e4).round() / 1e4))
                .filter(|(_, v)| *v != 0.0)
                .collect();
            (id, avg)
        })
        .collect()
}

pub fn retouch_uuid(id: &str) -> String {
    // Stable per retouch so re-installing replaces rather than duplicates.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in format!("retoucan-retouch-{id}").bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{:016X}{:016X}", h, h.rotate_left(29) ^ 0x5245_544F_5543_414E)
}

/// Write one adaptive preset per retouch into `<settings>/Retoucan/`.
pub fn write_retouch_presets(values: &BTreeMap<String, BTreeMap<String, f64>>) -> Result<PathBuf> {
    let dir = settings_dir().ok_or_else(|| anyhow::anyhow!("no settings folder"))?.join("Retoucan");
    std::fs::create_dir_all(&dir)?;
    for (n, r) in RETOUCH.iter().enumerate() {
        let locals: BTreeMap<String, f64> = values
            .get(r.id)
            .cloned()
            .unwrap_or_else(|| r.defaults.iter().map(|(k, v)| (k.to_string(), *v)).collect());
        std::fs::write(dir.join(format!("Retoucan - {}.xmp", r.label)), retouch_xmp(r, &locals, n))?;
    }
    Ok(dir)
}

fn retouch_xmp(r: &Retouch, locals: &BTreeMap<String, f64>, sort: usize) -> String {
    let uuid = retouch_uuid(r.id);
    let local_attrs: String = locals.iter().map(|(k, v)| format!("       crs:{k}=\"{v}\"\n")).collect();
    let masks: String = r
        .masks
        .iter()
        .enumerate()
        .map(|(i, (sub, cat, name))| {
            let cat = cat.map(|c| format!("         crs:MaskSubCategoryID=\"{c}\"\n")).unwrap_or_default();
            format!(
                "        <rdf:li\n         crs:What=\"Mask/Image\"\n         crs:MaskActive=\"true\"\n         crs:MaskName=\"{name}\"\n         crs:MaskBlendMode=\"0\"\n         crs:MaskInverted=\"false\"\n         crs:MaskSyncID=\"{}\"\n         crs:MaskValue=\"1\"\n         crs:MaskVersion=\"1\"\n         crs:MaskSubType=\"{sub}\"\n{cat}         crs:ReferencePoint=\"0.500000 0.500000\"\n         crs:ErrorReason=\"0\"/>\n",
                retouch_uuid(&format!("{}-mask-{i}", r.id))
            )
        })
        .collect();
    format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Retoucan">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:PresetType="Normal"
   crs:UUID="{uuid}"
   crs:SupportsAmount2="True"
   crs:SupportsAmount="True"
   crs:SupportsColor="True"
   crs:SupportsMonochrome="True"
   crs:SupportsHighDynamicRange="True"
   crs:SupportsNormalDynamicRange="True"
   crs:SupportsSceneReferred="True"
   crs:SupportsOutputReferred="True"
   crs:RequiresRGBTables="False"
   crs:CameraModelRestriction=""
   crs:Copyright="Retoucan (MIT)"
   crs:ContactInfo="https://github.com/joemalott/retoucan"
   crs:Version="15.0"
   crs:CompatibleVersion="251658240"
   crs:ProcessVersion="11.0"
   crs:HasSettings="True">
   <crs:Name>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">Retoucan - {label}</rdf:li>
    </rdf:Alt>
   </crs:Name>
   <crs:ShortName>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">{label}</rdf:li>
    </rdf:Alt>
   </crs:ShortName>
   <crs:SortName>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">Retoucan {sort:02}</rdf:li>
    </rdf:Alt>
   </crs:SortName>
   <crs:Group>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">Retoucan</rdf:li>
    </rdf:Alt>
   </crs:Group>
   <crs:MaskGroupBasedCorrections>
    <rdf:Seq>
     <rdf:li>
      <rdf:Description
       crs:What="Correction"
       crs:CorrectionAmount="1"
       crs:CorrectionActive="true"
       crs:CorrectionName="Retoucan {label}"
       crs:CorrectionSyncID="{sync}"
{local_attrs}      >
      <crs:CorrectionMasks>
       <rdf:Seq>
{masks}       </rdf:Seq>
      </crs:CorrectionMasks>
      </rdf:Description>
     </rdf:li>
    </rdf:Seq>
   </crs:MaskGroupBasedCorrections>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
"#,
        label = r.label,
        sync = retouch_uuid(&format!("{}-group", r.id)),
    )
}
