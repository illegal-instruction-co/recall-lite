---
description: how to use rememex MCP tools for semantic search instead of grep
---

## startup

// turbo
1. check index status:
```
rememex_index_status()
```
if `has_index` is false or `total_files` is 0, tell the user to open the main app and index some folders first.

// turbo
2. check recent changes for situational awareness:
```
rememex_diff(since: "2h")
```

## searching

use `rememex_search` with **natural language queries** — describe the concept, don't guess exact names.

```
rememex_search(query: "where GPS coordinates get converted to city names", file_extensions: ["rs"], top_k: 5)
```

use grep only when you need exact string matches (symbol occurrences, error messages, TODOs).

## reading results

after search gives you a path + snippet, read only the relevant lines:

```
rememex_read_file(path: "...", start_line: 50, end_line: 120)
```

never read entire files when search already told you where to look.

## discovering related code

when you find one relevant file and need its neighborhood:

```
rememex_related(path: "C:\\path\\to\\file.rs", top_k: 5)
```

returns semantically similar files — not imports, actual meaning similarity.

## containers

users can have multiple containers. check which one is active:

```
rememex_list_containers()
```

pass `container: "name"` to search a different one. don't assume there's only one.

## full agent instructions

for detailed patterns and advanced usage, read [AGENT.md](../../AGENT.md).