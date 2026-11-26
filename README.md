# WindowShot - Apple-style Window Screenshots for Windows

[![Release](https://img.shields.io/github/v/release/taylorivanoff/window-shot)](https://github.com/taylorivanoff/window-shot/releases)
[![Downloads](https://img.shields.io/github/downloads/taylorivanoff/window-shot/total)](https://github.com/taylorivanoff/window-shot/releases)
[![License](https://img.shields.io/github/license/taylorivanoff/window-shot)](LICENSE)

WindowShot is an open-source Windows utility that screenshots any visible window and exports a transparent PNG with Apple-style rounded corners, a real alpha cutout, and a soft drop shadow. It uses a Tauri 2 desktop UI with a Rust Win32 capture and rendering core.

Ideal for READMEs, documentation, marketing pages, and social posts where you want a clean macOS-style window instead of a rectangle pasted from the Windows clipboard.

<img width="978" height="698" alt="WindowShot desktop UI" src="docs/ui.png" />

## Features

- **Apple-style cutout** - antialiased rounded corners with transparent padding
- **Soft drop shadow** - adjustable blur, offset, opacity, and color
- **Live preview** - tune radius, padding, and shadow while you work
- **Capture modes** - `Auto`, `PrintWindow`, or screen-region capture for GPU-heavy apps
- **DPI-aware** - style values scale to the target window's DPI
- **Screen picker** - Windows Snip & Sketch-style overlay: the screen dims, the hovered window highlights, and a click captures it
- **Global shortcut** - press `Ctrl+Shift+S` from anywhere (`Alt+Shift+S` is a fallback)
- **Opens in Photos** - screen picks are saved to `Pictures\WindowShot` and opened in Microsoft Photos automatically
- **Window dropdown** - refresh and select from all capturable windows when you want manual control
- **Native save dialog** - choose where each PNG is written
- **Tauri tray scaffold** - built with the same framework, styles, and tray base as CmdDeck

## Installation

### Windows

1. Download the latest installer from [Releases](https://github.com/taylorivanoff/window-shot/releases)
2. Run the installer and follow the prompts
3. Launch **WindowShot** from the Start menu

WindowShot checks for updates on startup and once every 24 hours. You can also choose **Check for Updates** from the tray menu. Updates are downloaded from the latest GitHub Release.

WindowShot currently requires Windows because capture uses Win32 APIs directly.

### From source

See [Development](#development).

## Development

```bash
npm install
npm run dev
```

This syncs the shared styles, starts the local renderer, and launches the Tauri app in development mode.

### Linting

```bash
npm run lint
```

## Building

```bash
npm run build
```

The Windows NSIS installer is written to `src-tauri/target/release/bundle/nsis/`.

## Usage

### Quick capture

1. Press `Ctrl+Shift+S` (or click **Select on screen** in WindowShot)
2. Move the pointer over a window; it is highlighted with its title
3. Click the window to capture it
4. The PNG is saved to `Pictures\WindowShot` and opened in Photos
5. Press `Esc` or right-click to cancel

### Manual capture

1. Click **Refresh** to list open windows
2. Select the window you want to capture
3. Adjust corner radius, padding, and shadow settings
4. Click **Capture selected** (or press `Ctrl+Enter`)
5. Click **Save PNG** and choose a destination

### Import or paste an image

1. Click **Import image…** in the sidebar, or paste a screenshot into the preview with `Ctrl+V`
2. You can also drop a PNG, JPEG, or BMP file onto the preview
3. The image appears immediately with the current rounded corners and shadow
4. Adjust the style controls; the preview re-renders live from the original image
5. Click **Save PNG** to export the styled result

### Background color

- **Transparent** is the default; the checkerboard in the preview shows the real alpha
- Clear **Transparent** and pick a color to fill the exported PNG with a solid background
- The preview stage switches from the checkerboard to the chosen color so you can judge the result

### Capture modes

- **Auto** - try `PrintWindow` first, then fall back to a screen-region grab if the frame is blank
- **PrintWindow** - capture background or occluded windows where supported
- **Screen region** - most accurate colors, but the target window must be unobscured

## How it works

WindowShot calls `PrintWindow` with `PW_RENDERFULLCONTENT`, crops the result to the DWM extended frame bounds, applies a signed-distance rounded-corner alpha mask, blurs the shadow into a transparent or solid canvas, and writes the final PNG through the `image` crate. Imported and pasted images are decoded to RGBA and kept as the source, so style and background changes re-render the same image without losing quality. If `PrintWindow` returns a flat frame, `Auto` falls back to `BitBlt` from the screen device context.

## Releasing

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `src-tauri/Cargo.lock`
2. Keep the Minisign public key and `latest.json` endpoint in `src-tauri/tauri.conf.json` in sync with the `TAURI_SIGNING_PRIVATE_KEY` repository secret
3. Push to `master` - the Release workflow builds the signed NSIS installer and publishes a `v<version>` GitHub Release with the installer, `.sig`, and `latest.json` attached
4. You can also run the Release workflow manually from the Actions tab; versions that are already released are skipped

## Keywords

Apple-style window screenshot, transparent PNG, rounded corners, drop shadow, Win32 PrintWindow, window capture utility, Tauri desktop app, Rust Windows app

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT
