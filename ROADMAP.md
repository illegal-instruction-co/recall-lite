# roadmap

- ~~**MCP server**~~ done -- `recall-mcp` binary exposes search as tools over stdio. any MCP client (cursor, claude desktop, copilot) can use it out of the box
- ~~**file watcher**~~ done -- `notify` crate, OS-level events (zero CPU idle), 500ms debounce. auto re-embeds changed files, removes deleted ones. `reindex_all` now does delta instead of nuking the table like a maniac
- **agentic search** -- local LLM that can grep --> read --> reason --> answer in a loop. notebooklm but private
- **linux / mac** -- need cross-platform alternatives for OCR and mica backdrop
- **more file types** -- always

want something? open an issue.
