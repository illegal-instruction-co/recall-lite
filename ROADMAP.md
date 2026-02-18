# roadmap

- ~~**MCP server**~~ done -- `recall-mcp` binary exposes search as tools over stdio. any MCP client (cursor, claude desktop, copilot) can use it out of the box
- ~~**file watcher**~~ done -- `notify` crate, OS-level events (zero CPU idle), 500ms debounce. auto re-embeds changed files, removes deleted ones. `reindex_all` now does delta instead of nuking the table like a maniac
- **agentic search** -- local LLM that can grep --> read --> reason --> answer in a loop. notebooklm but private
- ~~**vibe coding / agent support**~~ done -- MCP server is now agent-optimized:
  - ~~bigger context per result~~ done -- `context_bytes` param, up to 10KB per snippet
  - ~~file type / path filtering~~ done -- `file_extensions` and `path_prefix` params on search
  - ~~configurable result count~~ done -- `top_k` param, 1-50 results
  - ~~agents can read files without leaving MCP~~ done -- `recall_read_file` with line ranges
  - ~~agents can browse project structure~~ done -- `recall_list_files` with filters
  - ~~agents can check index health~~ done -- `recall_index_status`
  - tree-sitter based chunking. split on function/class boundaries instead of byte counts
  - ~~agent-triggered indexing~~ **intentionally excluded** -- indexing takes minutes and agents shouldn't silently index folders. users pick what to index from the GUI. that's a security boundary, not a missing feature
  - the goal: make recall-lite the local private alternative to greptile/sourcegraph for AI-assisted coding
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
- **more file types** -- always

want something? open an issue.
