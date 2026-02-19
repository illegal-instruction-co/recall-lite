# best practices

you installed rememex, indexed some folders, ran a search. cool. here's how to not waste your time and actually get good results.

## containers: think of them as brains

don't throw everything into one container. you'll search for "database migration" and get results from your work project, your side project, and that tutorial you downloaded three years ago.

```
Work        → C:\Projects\company-stuff
Personal    → D:\Notes, D:\Photos
Research    → C:\Papers, C:\Experiments
```

each container = isolated index. separate embeddings, separate search. your weekend side project won't pollute your work search results.

one exception: if you're working on a monorepo and want cross-project search, one container is fine. use your brain.

## queries: describe meaning, not keywords

this is the most common mistake. people treat rememex like grep. it's not grep.

```
BAD:  "config"
BAD:  "handleClick"
BAD:  "fn parse"

GOOD: "where does the app load its configuration on startup"
GOOD: "the click handler that opens the settings panel"
GOOD: "parsing logic for user input validation"
```

the embedding model understands concepts. "summer morning" finds photos from july at 8am. "database connection cleanup" finds the pool shutdown code even if the code never uses the word "cleanup".

you're talking to a model that read the internet, not ctrl+f.

## chunk size: don't touch it (probably)

defaults are tuned per file type:
- code: 1200 bytes
- docs: 800 bytes
- configs: 600 bytes

the embedding model has a ~512 token limit. bigger chunks = more truncation = worse search. smaller chunks = more noise = also worse search.

if you change it, you'll spend 30 minutes wondering why search got worse. then you'll set it back. we've all been there.

the only time to change it: you have very long functions (>100 lines) and want to capture more context per chunk. bump to 1500-1800. don't go above 2000.

## .rcignore: your first line of defense

drop a `.rcignore` in any indexed folder. same syntax as `.gitignore`.

things you should ignore:
```
node_modules/
dist/
build/
.git/
*.min.js
*.min.css
*.map
*.lock
package-lock.json
yarn.lock
*.sqlite
*.db
```

the app ships with sane defaults, but if you're indexing a project with 50,000 generated files, add your own. less junk indexed = faster indexing = better search = less RAM.

## the reranker: 1GB of "worth it"

the JINA cross-encoder reranker is on by default. it uses ~1GB RAM but dramatically improves result ordering. leave it on.

when to turn it off:
- you're on a machine with <8GB RAM and every megabyte counts
- you're using high-quality remote embeddings (OpenAI `text-embedding-3-large`, Gemini). these models are good enough that the reranker adds marginal value
- you're indexing tiny projects where top-5 results are obvious anyway

```json
{
  "use_reranker": false
}
```

## local vs remote embeddings: the tradeoff

**local (default):** free, private, no API keys, decent quality. uses Multilingual-E5-Base via ONNX. works offline. ~2GB model download on first run.

**remote:** better embedding quality (especially for code), costs money, sends text chunks to an API. your files stay local — only the chunked text is sent for embedding.

when remote makes sense:
- large codebases where you need precise code search
- multilingual content beyond what E5 handles well
- you already have an OpenAI/Gemini API key and don't care about the cost
- you're running on a low-spec machine and local inference is too slow

when local is better:
- air-gapped environments
- you're cheap (respect)
- you're indexing personal stuff and don't want chunks on anyone's servers
- the defaults work fine for your use case (they usually do)

don't mix local and remote in the same container. each container snapshots its provider at creation time. if you want to try remote, create a new container. switching providers on an existing container means reindexing everything from scratch — the old vectors have different dimensions.

## RAM: the elephant in the room

rememex uses real memory. here's the breakdown:

| component | RAM | when |
|-----------|-----|------|
| embedding model | ~1.1 GB | always loaded |
| reranker | ~1 GB | always loaded (unless disabled) |
| indexing buffer | 0.5-2 GB | during indexing only |
| lancedb | varies | depends on index size |

peak usage happens during initial indexing. once it's done, it drops and stays stable. don't panic when you see 3-4GB during first index — it's temporary.

tips to reduce memory:
- disable the reranker if you're tight
- use `AllMiniLML6V2` instead of `MultilingualE5Base` (smaller model, still decent)
- index folders incrementally, not everything at once
- close and reopen the app after initial indexing — some buffers get freed

## annotations: the sleeper feature

most people skip this. don't.

annotations are searchable notes attached to files. they get embedded just like file content. leave a note on a file → it shows up in future searches.

from the UI:
- click a file → add annotation → type your note

from MCP (for AI agents):
```
rememex_annotate(path: "...", note: "this file handles auth token refresh. critical path, don't break it.")
```

good annotation habits:
- mark files with known bugs or gotchas
- explain non-obvious architecture decisions ("this looks redundant but it's intentional because...")
- tag files by domain ("payment processing", "user onboarding flow")
- leave warnings on fragile code

annotations persist across sessions and conversations. they're your institutional memory.

## OCR: it's better than you think

rememex OCRs images automatically via Windows UWP engine. screenshots, diagrams, photos of whiteboards — all searchable.

it also reads EXIF data:
- GPS coordinates → reverse geocoded to city names. search "photos from paris" and it works
- timestamps → human language. "summer morning" finds photos from june-august taken before 10am

to get the most out of it:
- make sure images are in indexed folders
- don't `.rcignore` your screenshot folders
- search with natural language, not filenames. "screenshot of the error message" > "error_2024.png"

## the hotkey: muscle memory or nothing

default is `Alt+Space`. if you don't use it within 3 days, you'll forget it exists and go back to `Ctrl+Shift+F` like a caveman.

rebind it to something you'll actually press. `Ctrl+Space` if your IDE doesn't fight you. `Win+S` if you want to replace windows search (you do). pick something and commit.

```json
{
  "hotkey": "Ctrl+Space"
}
```

## file watcher: it just works (mostly)

the watcher picks up file changes in real time. create a file, edit a file, delete a file — the index updates automatically.

things that trip it up:
- renaming a folder with 10,000 files → storm of events, might take a sec
- network drives → sometimes the OS doesn't fire filesystem events. local drives only
- WSL filesystems → same problem. index from the windows side if possible
- git checkout that changes 500 files → it handles it, but give it a moment

if search results feel stale, just restart the app. the watcher re-scans everything on startup.

## when search doesn't find what you want

1. **rephrase the query.** use different words, be more specific. "error handling" → "what happens when the API returns a 500 status code"
2. **check the container.** you might be searching the wrong one
3. **check the file type.** is the file extension in the supported list? if not, add it to `extra_extensions`
4. **check .rcignore.** you might be ignoring the folder
5. **wait for indexing.** if you just added a folder, give it a minute to index
6. **read the logs.** `%AppData%\com.rememex.app\rememex.log` tells you what happened

if none of that helps, open an issue. include the query, what you expected, and what you got.
