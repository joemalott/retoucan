<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" alt="Retoucan app icon, a toucan on a teal square">
</p>

<h1 align="center">Retoucan</h1>

<p align="center">
  <strong>Free, open-source AI photo editing for Lightroom Classic.</strong><br>
  Learns your editing style from your own catalogs and gives a whole shoot a first pass, on your own computer.<br>
  A local alternative to Imagen AI and Aftershoot for wedding, portrait and event photographers.
</p>

<p align="center">
  <a href="https://github.com/joemalott/retoucan/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/joemalott/retoucan"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/github/license/joemalott/retoucan"></a>
  <img alt="macOS and Windows" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey">
</p>

<p align="center">
  <a href="https://github.com/joemalott/retoucan/releases/latest"><strong>Download for macOS or Windows</strong></a> · <a href="https://retoucan.com">retoucan.com</a>
</p>

## What it does

You import and cull in Lightroom Classic as usual. Retoucan reads your flags, stars and color labels, edits the photos you picked, and writes the results back into Lightroom as normal develop settings. Every slider and mask stays editable, and one click undoes the lot.

- **Learns your style.** Point it at a catalog you've already edited. It trains in about a minute and predicts exposure, white balance (temperature and tint) and tone for each new photo.
- **Applies a look.** Retoucan scans the presets and profiles already installed in Lightroom Classic, including LUT-based ones, and adds ten film looks of its own. There's no upload: to use a new preset, import it into Lightroom first. A bare `.cube` LUT has to be made into a Lightroom profile or preset before Retoucan can see it. Preview the look on sample photos and set the strength before anything touches your catalog.
- **AI masks and retouching.** Adds Lightroom's own AI masks for subject, background, skin, teeth, eyes and sky with gentle adjustments, and removes masks that found nothing (no faces, no sky).
- **Test batch first.** Tries the edit on a spread of samples from the shoot. If you like them, it edits the rest; if not, it undoes them.
- **Batch editing for big shoots.** Handles hundreds of RAW photos in one go, with progress and time remaining. Edited photos get a `Retoucan` keyword, and any that need a second look go in a collection.
- **Private.** No account, no upload, no per-photo fee. Your photos and catalogs never leave your computer.

## How it works

1. **Cull in Lightroom Classic** with flags, stars or color labels.
2. **Choose photos** in Retoucan. It reads the open catalog and filters by flag, rating, label and folder.
3. **Pick a look** and preview it on samples from the shoot.
4. **Pick retouching**: which masks to add and how strong.
5. **Review and send.** Retoucan edits a test batch, then the rest. The companion Lightroom plugin applies the edits, so they're ordinary Lightroom settings you can keep tweaking.

There's also a global shortcut to edit the photos selected in Lightroom: Control+Option+Command+E on macOS, Control+Alt+Shift+E on Windows.

## Install

1. Download the installer for your computer from the [latest release](https://github.com/joemalott/retoucan/releases/latest): `aarch64.dmg` for Apple Silicon Macs, `x64.dmg` for Intel Macs, `x64-setup.exe` for Windows.
2. Open Retoucan and go to **Setup → Lightroom** to install the Lightroom Classic plugin.
3. Restart Lightroom Classic.

The builds aren't code-signed yet. On macOS, right-click the app and choose **Open** the first time. On Windows, choose **More info → Run anyway**.

**Requirements:** Lightroom Classic 13.3 or later (for AI masks), macOS or Windows. The Windows build hasn't been tested with Lightroom yet; reports are welcome.

## Your presets stay yours

Retoucan uses the presets and looks already installed in your Lightroom, reading them from your computer when it needs them. It never copies, uploads or redistributes them. The only looks bundled with Retoucan are the ten in `looks/`, which were written for this project and are MIT licensed like the rest of it.

## Command line

The `retoucan` binary also runs from a terminal, which is handy for scripting:

```
retoucan train <catalog.lrcat>     # learn a style from an edited catalog
retoucan edit <photos>             # edit photos through the Lightroom plugin
retoucan undo|cleanup|check <photos>
retoucan looks                     # list available looks
retoucan doctor                    # check the Lightroom connection and plugin
```

## Building from source

Retoucan is a [Tauri](https://tauri.app) app with a Rust backend, a Svelte frontend and a Lua plugin for Lightroom Classic. You need [Rust](https://rustup.rs) and [Bun](https://bun.sh).

```
bun install
bun run tauri dev
```

## Status

Early development, tested on a full 700-photo wedding. Next up: splitting a shoot into scenes by time and light, with a look per scene and consistent edits within each one.

Issues and pull requests are welcome.

## License

MIT. Retoucan isn't affiliated with Adobe, Imagen or Aftershoot. Lightroom is a trademark of Adobe.
