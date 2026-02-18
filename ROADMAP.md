# Roadmap

## Done

- ~~**Native Rust UI**~~ done -- eframe/egui replaces Tauri + React entirely. Pure Rust, no npm, no Node, no Vite. Windows 11 Mica via wgpu. Spotlight window behavior with hide-on-unfocus.
- ~~**MCP server**~~ done -- `recall-mcp.exe` exposes search as tools over stdio. Any MCP client (Cursor, Claude Desktop, VS Code Copilot) works out of the box.
- ~~**File watcher**~~ done -- `notify` crate, OS-level events, zero CPU at idle, 500 ms debounce. Auto re-indexes changed files, removes deleted ones.
- ~~**Vibe coding / agent support**~~ done -- MCP server is agent-optimized:
  - ~~bigger context per result~~ done -- `context_bytes` param, up to 10 KB per snippet
  - ~~file type / path filtering~~ done -- `file_extensions` and `path_prefix` on search
  - ~~configurable result count~~ done -- `top_k` param, 1-50 results
  - ~~agents can read files without leaving MCP~~ done -- `recall_read_file` with line ranges
  - ~~agents can browse project structure~~ done -- `recall_list_files` with filters
  - ~~agents can check index health~~ done -- `recall_index_status`

## Planned

- **tree-sitter chunking** -- split on actual function / class boundaries instead of byte counts. Better chunks = better search quality, especially for large files.
- **agentic search** -- local LLM that can search → read → reason → answer in a loop. NotebookLM, but private.
- **Linux / macOS** -- the blocking items are OCR (Windows.Media.Ocr is WinRT-only) and Mica (Windows-only). Cross-platform alternatives exist for both.
- **More file types** -- always.

## Intentionally excluded

- **Agent-triggered indexing** -- indexing takes minutes and agents should not silently index folders. The user picks what to index from the GUI. That is a security boundary, not a missing feature.

---

Want something? Open an issue.
