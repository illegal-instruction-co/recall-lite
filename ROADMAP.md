# roadmap

- **vibe coding / agent support**
  - tree-sitter based chunking. split on function/class boundaries instead of byte counts
  - the goal: make rememex the local private alternative to greptile/sourcegraph for AI-assisted coding
- **macOS** -- next priority after current roadmap items:
  - OCR: `Vision.framework` (built-in, no deps)
  - backdrop: vibrancy already supported via `window-vibrancy`
  - packaging: `.dmg` installer + homebrew formula
  - global hotkey: already cross-platform via tauri plugin
- **linux** -- after macOS:
  - OCR: `tesseract` (widely available, package manager install)
  - backdrop: skip or basic transparency (no native equivalent)
  - packaging: `.AppImage` + `.deb` + flatpak
  - global hotkey: X11/Wayland support via tauri plugin
- **content browser** -- browse indexed files visually like a file manager. image thumbnails, video previews, PDF first page, code with syntax highlighting. not just search -- let people explore their stuff naturally
- **more file types** -- always

want something? open an issue.
