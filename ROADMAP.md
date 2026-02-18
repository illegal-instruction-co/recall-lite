# roadmap

- ~~**MCP server**~~ done -- `recall-mcp` binary exposes search as tools over stdio. any MCP client (cursor, claude desktop, copilot) can use it out of the box
- ~~**file watcher**~~ done -- `notify` crate, OS-level events (zero CPU idle), 500ms debounce. auto re-embeds changed files, removes deleted ones. `reindex_all` now does delta instead of nuking the table like a maniac
- **agentic search** -- local LLM that can grep --> read --> reason --> answer in a loop. notebooklm but private
- **vibe coding / agent support** -- the MCP server works but agents need more:
  - bigger context per result. 300 bytes is nothing, agents want whole functions
  - file type / path filtering in search queries (`"only .rs files"`, `"only src/"`)
  - tree-sitter based chunking. split on function/class boundaries instead of byte counts
  - configurable result count (default 3 is too few for agents, 10-15 is the sweet spot)
  - realtime index refresh after file changes so agents don't search stale data
  - the goal: make recall-lite the local private alternative to greptile/sourcegraph for AI-assisted coding
- **linux / mac** -- need cross-platform alternatives for OCR and mica backdrop
- **more file types** -- always

want something? open an issue.
