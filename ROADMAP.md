# roadmap

- ~~**MCP server**~~ done -- `rememex-mcp` binary exposes search as tools over stdio. any MCP client (cursor, claude desktop, copilot) can use it out of the box
- ~~**file watcher**~~ done -- `notify` crate, OS-level events (zero CPU idle), 500ms debounce. auto re-embeds changed files, removes deleted ones. `reindex_all` now does delta instead of nuking the table like a maniac
- ~~**vibe coding / agent support**~~ done -- MCP server is now agent-optimized:
  - ~~bigger context per result~~ done -- `context_bytes` param, up to 10KB per snippet
  - ~~file type / path filtering~~ done -- `file_extensions` and `path_prefix` params on search
  - ~~configurable result count~~ done -- `top_k` param, 1-50 results
  - ~~agents can read files without leaving MCP~~ done -- `rememex_read_file` with line ranges
  - ~~agents can browse project structure~~ done -- `rememex_list_files` with filters
  - ~~agents can check index health~~ done -- `rememex_index_status`
  - tree-sitter based chunking. split on function/class boundaries instead of byte counts
  - ~~agent-triggered indexing~~ **intentionally excluded** -- indexing takes minutes and agents shouldn't silently index folders. users pick what to index from the GUI. that's a security boundary, not a missing feature
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
- ~~**git history indexing**~~ done -- appends last 50 commit messages to file content before embedding. search "why was this changed" and it finds the commit. no git? no problem, just skips
- ~~**cloud embedding providers (optional)**~~ done -- hybrid approach: local stays default, but users can plug in OpenAI / Gemini / Cohere / custom API for embedding + reranking. benefits:
  - ~~zero RAM overhead~~ done -- no model loading, works on low-spec machines
  - ~~better quality~~ done -- `text-embedding-3-large` or Gemini embeddings beat local fastembed
  - ~~trait abstraction~~ done -- `EmbeddingProvider` + `RerankerProvider` -- local model implements it, cloud providers are plug-and-play
  - ~~user brings their own API key~~ done -- opts into the privacy trade-off consciously
  - ~~default is still 100% local~~ done -- nothing changes for existing users
- ~~**per-container embedding model**~~ done -- each container snapshots its embedding provider at creation time. switching containers auto-loads the correct provider (local or remote). sidebar shows provider label per container. backward compatible: existing containers fall back to global provider
- ~~**custom title bar**~~ done -- custom drag area with minimize + close (hide to tray) buttons. Windows 11 style, red close hover
- ~~**UI polish**~~ done -- status bar padding fixed, hotkey shortcut label now reads from config instead of hardcoded "Alt+Space"
- **content browser** -- browse indexed files visually like a file manager. image thumbnails, video previews, PDF first page, code with syntax highlighting. not just search -- let people explore their stuff naturally
- **more file types** -- always

want something? open an issue.
