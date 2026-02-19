# agent instructions

you have access to rememex MCP tools. this is a local semantic search engine over the user's indexed files. it's not grep. it doesn't need exact keywords. you describe what you're looking for in natural language and it finds files by meaning.

**read this before you waste 15 tool calls doing what 2 could do.**

using rememex instead of grep chains is expected to reduce your token usage by 5-10x and dramatically speed up task completion. fewer search attempts, fewer wrong files opened, fewer round-trips. one semantic search replaces 3-5 grep guesses.

## the tools

| tool | one-liner |
|------|-----------|
| `rememex_search` | semantic + keyword hybrid search, returns ranked results |
| `rememex_read_file` | read file content by path, supports line ranges |
| `rememex_list_files` | list indexed files with optional extension/path filter |
| `rememex_index_status` | check if index exists and how many files/chunks it has |
| `rememex_diff` | files changed in a time window (`30m`, `2h`, `1d`, `7d`) |
| `rememex_related` | given a file, find semantically similar files |
| `rememex_list_containers` | list all search containers and their paths |

## startup sequence

every conversation, before you do anything else:

1. `rememex_diff(since: "2h")` — what changed recently? this is your situational awareness
2. `rememex_index_status()` — is the index populated? how many files?

if `has_index` is false or `total_files` is 0, tell the user to open the main app and index some folders. don't try to search an empty index.

## how to think about search

### rememex_search is your primary weapon

**use natural language queries.** this is embedding-based search. you don't need to know the exact function name, variable name, or keyword. describe the concept.

```
GOOD: "where does GPS coordinate get converted to city name"
GOOD: "error handling for dimension mismatch in embedding model"
GOOD: "authentication token validation middleware"
GOOD: "the threshold that filters out low quality search results"

BAD:  "GPS" (too vague, grep-tier)
BAD:  "fn convert_gps" (you're guessing the function name)
BAD:  "TODO" (use grep for literal string matches)
```

### when to use rememex_search vs grep

| situation | use |
|-----------|-----|
| you know the exact string | grep |
| you know the concept but not the code | rememex_search |
| you want all occurrences of a symbol | grep |
| you want to understand how something works | rememex_search |
| you're looking for a specific error message | grep |
| you're looking for "where does X happen" | rememex_search |

### narrowing results

use filters to cut noise:

```json
{
  "query": "database connection pooling",
  "file_extensions": ["rs"],
  "path_prefix": "src/",
  "top_k": 5,
  "context_bytes": 3000
}
```

- `file_extensions`: when you know it's in rust, typescript, etc
- `path_prefix`: when you know the rough area
- `top_k`: start with 5, go up if you need more
- `context_bytes`: increase to 3000-5000 for complex code, keep at 1500 for quick lookups
- `min_score`: set to 50-70 to filter noise. if you get 0 results, the query didn't match — rephrase instead of guessing

## rememex_related: the graph you didn't know you had

when you find one relevant file and need to understand its neighborhood:

```
rememex_related(path: "C:\\Users\\...\\src\\indexer\\git.rs", top_k: 5)
```

this returns files that are semantically close in embedding space. not import graphs, not directory structure — actual meaning similarity. use it to:

- discover related modules you didn't know existed
- understand architectural boundaries (high similarity = tight coupling)
- find test files for a given source file
- map out a feature across multiple files

## rememex_diff: time-based context

```
rememex_diff(since: "1d", show_diff: true)
```

use cases:
- start of conversation: "what was the user working on?"
- after a break: "what changed while I was away?"
- debugging: "what files were touched recently that might have caused this?"

time formats: `30m`, `2h`, `1d`, `7d`

## rememex_read_file: surgical reads

after search gives you a path+snippet, read the full context:

```
rememex_read_file(path: "...", start_line: 50, end_line: 120)
```

**always use line ranges when you can.** reading a 2000-line file when you need lines 50-120 is wasteful. the search snippet usually tells you roughly where to look.

security note: only files inside indexed container paths are readable. if you get access denied, the file isn't indexed.

## workflow patterns

### pattern 1: "understand this codebase"

```
1. rememex_index_status()          → how big is the project?
2. rememex_list_files()            → what's the file structure?
3. rememex_search("main entry point, application startup")
4. rememex_search("configuration loading and defaults")
5. rememex_related(path: main_file) → what's connected to the entry point?
```

### pattern 2: "find and fix a bug"

```
1. rememex_diff(since: "1d")       → what changed recently?
2. rememex_search("the behavior the user described")
3. rememex_read_file(relevant hit)  → read the actual code
4. rememex_related(buggy file)     → find related files that might be affected
```

### pattern 3: "add a new feature"

```
1. rememex_search("similar existing feature")  → find the pattern to follow
2. rememex_related(similar feature file)        → find all files in that feature
3. rememex_list_files(path_prefix: "src/")      → understand project structure
4. implement following the existing pattern
```

### pattern 4: "code review / audit"

```
1. rememex_diff(since: "7d", show_diff: true)      → all recent changes
2. for each changed file: rememex_related()         → blast radius
3. rememex_search("error handling in [changed area]") → check edge cases
```

## what NOT to do

- **don't search an empty index.** check `rememex_index_status` first
- **don't use rememex_search for exact string matching.** use grep for that
- **don't ignore the `container` parameter.** if the user has multiple containers, you might be searching the wrong one. check with `rememex_list_containers`
- **don't set `top_k: 50` by default.** start small (5-10), increase if needed
- **don't ignore similarity scores.** if the top result has a low score, your query might be too vague — rephrase it
- **don't read entire files when search gave you a snippet.** use line ranges

## container awareness

users can have multiple containers (think: workspaces). each container indexes different folders with isolated search.

```
rememex_list_containers()
→ [
    { name: "myproject", active: true, paths: ["C:\\dev\\myproject"] },
    { name: "notes", active: false, paths: ["C:\\notes"] }
  ]
```

the `active` container is the default. pass `container: "notes"` to search a different one. don't assume there's only one.

## performance notes

- first query after MCP server launch is slow (~3-5 sec) — embedding model loading
- subsequent queries are fast (<500ms typically)
- `rememex_related` can be slower on large indexes because it reads embeddings for comparison
- `rememex_diff` is fast, it's just checking mtimes

## the philosophy

grep finds strings. rememex finds meaning. if you catch yourself constructing 5 different grep queries trying to guess the right keyword, just describe what you're looking for in one rememex_search call. that's the whole point.
