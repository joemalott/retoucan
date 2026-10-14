//! Command-line mode of the Retoucan app. The Lightroom plugin and MCP
//! clients launch the same binary with a subcommand.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::engine::{self, EditOptions};

pub const COMMANDS: &[&str] = &["mcp", "edit-selection", "edit", "train", "install-plugin", "styles", "looks", "status", "doctor", "undo", "cleanup", "check"];

/// Returns `None` when the arguments aren't a CLI command (launch the UI).
pub fn run(args: &[String]) -> Option<Result<()>> {
    let cmd = args.get(1)?;
    if !COMMANDS.contains(&cmd.as_str()) {
        return None;
    }
    Some(dispatch(cmd, &args[2..]))
}

fn dispatch(cmd: &str, rest: &[String]) -> Result<()> {
    match cmd {
        "mcp" => crate::mcp::serve(),
        "edit-selection" => {
            let lr = engine::lightroom_state();
            if lr.selected.is_empty() {
                bail!("nothing selected in Lightroom");
            }
            let paths: Vec<PathBuf> = lr.selected.iter().map(PathBuf::from).collect();
            let (job, _) = engine::edit(&paths, &EditOptions::default())?;
            println!("{}", job.id);
            Ok(())
        }
        "edit" => {
            let paths: Vec<PathBuf> = rest.iter().map(PathBuf::from).collect();
            if paths.is_empty() {
                bail!("usage: retoucan edit <photo>...");
            }
            let (job, edits) = engine::edit(&paths, &EditOptions::default())?;
            for e in edits {
                println!("{}  {}", e.path.display(), e.error.unwrap_or_else(|| serde_json::to_string(&e.correction).unwrap()));
            }
            println!("queued job {}", job.id);
            Ok(())
        }
        "train" => {
            let (Some(cat), Some(name)) = (rest.first(), rest.get(1)) else {
                bail!("usage: retoucan train <catalog.lrcat> <style name>");
            };
            let s = engine::train_style(Path::new(cat), name, &|p| eprint!("\r{} {}/{}   ", p.stage, p.done, p.total))?;
            eprintln!();
            println!("{}", serde_json::to_string_pretty(&s)?);
            Ok(())
        }
        "install-plugin" => {
            println!("installed to {}", engine::install_plugin()?.display());
            Ok(())
        }
        "styles" => {
            println!("{}", serde_json::to_string_pretty(&engine::list_styles())?);
            Ok(())
        }
        "looks" => {
            for l in engine::list_looks() {
                println!("{}  [{}] {} ({})", l.uuid, l.group, l.name, l.kind);
            }
            Ok(())
        }
        "undo" | "cleanup" | "check" => {
            let paths: Vec<PathBuf> = rest.iter().map(PathBuf::from).collect();
            if paths.is_empty() {
                bail!("usage: retoucan {cmd} <photo>...");
            }
            println!("queued job {}", engine::queue_action(cmd, &paths)?.id);
            Ok(())
        }
        "doctor" => {
            println!("{}", serde_json::to_string_pretty(&engine::doctor())?);
            Ok(())
        }
        "status" => {
            println!("{}", serde_json::to_string_pretty(&engine::lightroom_state())?);
            println!("{}", serde_json::to_string_pretty(&engine::load_config())?);
            Ok(())
        }
        _ => unreachable!(),
    }
}
