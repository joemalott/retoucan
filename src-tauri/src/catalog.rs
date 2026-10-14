//! Read-only access to a Lightroom Classic catalog (.lrcat, SQLite).

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::lrlua;

#[derive(Debug, Clone, Serialize)]
pub struct CatalogPhoto {
    pub id: i64,
    pub path: PathBuf,
    pub exists: bool,
    pub pick: f64,
    pub rating: Option<f64>,
    pub capture_time: Option<String>,
    /// Lightroom color label ("Red", "Green", ...), if any.
    pub label: Option<String>,
    /// Folder path relative to its root, e.g. "Main Cam/".
    pub folder: String,
    /// Tagged with the "Retoucan" keyword, i.e. already edited by Retoucan.
    pub retoucan: bool,
    /// Parsed develop settings; `Value::Null` when the photo has none.
    pub settings: Value,
}

/// Open read-only. While Lightroom has the catalog open, its newest changes
/// (flags, edits) sit in the WAL, so read through it; Lightroom doesn't lock
/// readers out. If that fails, fall back to `immutable=1`, which takes no
/// locks at all but only sees what Lightroom has checkpointed.
pub fn open(path: &Path) -> Result<Connection> {
    let escaped = path.to_string_lossy().replace('?', "%3f").replace('#', "%23");
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let live = Connection::open_with_flags(format!("file:{escaped}?mode=ro"), flags)
        .and_then(|c| c.query_row("SELECT count(*) FROM Adobe_images", [], |_| Ok(())).map(|_| c));
    match live {
        Ok(c) => Ok(c),
        Err(_) => Connection::open_with_flags(format!("file:{escaped}?mode=ro&immutable=1"), flags)
            .with_context(|| format!("opening catalog {}", path.display())),
    }
}

pub fn photos(catalog: &Path) -> Result<Vec<CatalogPhoto>> {
    let conn = open(catalog)?;
    let catalog_dir = catalog.parent().unwrap_or(Path::new("."));
    let mut stmt = conn.prepare(
        "SELECT i.id_local, r.absolutePath, r.relativePathFromCatalog, fo.pathFromRoot,
                f.baseName, f.extension, i.pick, i.rating, i.captureTime, d.text,
                NULLIF(i.colorLabels, ''),
                EXISTS (SELECT 1 FROM AgLibraryKeywordImage ki
                        JOIN AgLibraryKeyword k ON k.id_local = ki.tag
                        WHERE ki.image = i.id_local AND k.name = 'Retoucan')
         FROM Adobe_images i
         JOIN AgLibraryFile f ON f.id_local = i.rootFile
         JOIN AgLibraryFolder fo ON fo.id_local = f.folder
         JOIN AgLibraryRootFolder r ON r.id_local = fo.rootFolder
         LEFT JOIN Adobe_imageDevelopSettings d ON d.image = i.id_local
         WHERE i.masterImage IS NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<f64>>(6)?,
            r.get::<_, Option<f64>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<String>>(9)?,
            r.get::<_, Option<String>>(10)?,
            r.get::<_, bool>(11)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id, abs, rel, folder, base, ext, pick, rating, capture, text, label, retoucan) = row?;
        let folder = folder.unwrap_or_default();
        let file = format!("{base}.{ext}");
        // Prefer the absolute root; fall back to the catalog-relative one,
        // which is what exported catalogs and moved drives rely on.
        let mut candidates = Vec::new();
        if let Some(a) = abs {
            candidates.push(PathBuf::from(a).join(&folder).join(&file));
        }
        if let Some(r) = rel {
            candidates.push(catalog_dir.join(r).join(&folder).join(&file));
        }
        let path = candidates
            .iter()
            .find(|p| p.exists())
            .cloned()
            .or_else(|| candidates.first().cloned())
            .unwrap_or_else(|| PathBuf::from(&file));
        let settings = text
            .as_deref()
            .and_then(|t| lrlua::parse(t).ok())
            .unwrap_or(Value::Null);
        out.push(CatalogPhoto {
            id,
            exists: path.exists(),
            path,
            pick: pick.unwrap_or(0.0),
            rating,
            capture_time: capture,
            label,
            folder: folder.clone(),
            retoucan,
            settings,
        });
    }
    Ok(out)
}
