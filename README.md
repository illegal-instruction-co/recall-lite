# Recall Lite (local-mind)

Windows search sucks. Copilot is creepy. I needed something that finds my sh*t without sending my screen to the cloud.

So I built this.

![Demo](demo.gif)

## What is this?
It's a **local-first** semantic search engine.
- indexes your files (PDF, txt, md, code)
- stores vectors locally (LanceDB)
- runs a tiny BERT model on your CPU (fastembed-rs)
- **0% data leaves your machine.**

## Why?
I have thousands of PDFs and notes. I don't remember filenames. I remember "that invoice about server costs" or "the rust code where I fixed the memory leak".
Typical regex search fails here. Vector search doesn't.

## Tech Stack (The good stuff)
- **Frontend**: React + TypeScript + Tailwind (because it works)
- **Backend**: Rust (fast af)
- **Vectors**: [LanceDB](https://lancedb.com/) (embedded, no docker junk)
- **Model**: `Multilingual-E5-small` (runs on a potato)
- **UI**: Windows 11 Fluent / Mica (looks native)

## How to run
You need Rust and Node installed.

```bash
# install deps
npm install

# run dev (it will download the model on first run, ~100mb)
npm run tauri dev

# build release
npm run tauri build
```

## Usage
- **Alt + Space**: Toggle the search bar instantly (Global Shortcut).
- **Ctrl + O**: Index a new folder.
- **Esc**: Clear search or hide window.

## Performance
- Tested on 10k files.
- Indexing takes a bit (it's CPU bound).
- Search is <50ms.

## disclaimer
code is a bit messy aka "vibe coding". it works on my machine.
pull requests welcome if you want to fix my terrible react hooks.

## License
MIT. Do whatever.
