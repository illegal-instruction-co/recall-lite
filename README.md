# Recall Lite (local-mind)

Windows search sucks. Copilot is creepy. I needed something that finds my sh*t without sending my screen to the cloud.

So I built this.

![SetupContainer](https://github.com/user-attachments/assets/75573638-2fda-4a68-bf61-30aaf5d2ad67)

![Search](https://github.com/user-attachments/assets/98407df3-1984-4f3d-9cd5-21cbfdc4cb85)

## What is this?
It's a **local-first** semantic search engine.
- indexes your files (PDF, images, txt, md, code — 50+ formats)
- **OCR support** — extracts text from PNG, JPG, BMP, TIFF via Windows built-in OCR
- stores vectors locally (LanceDB)
- hybrid search: vector + full-text + JINA reranker
- **semantic containers** — isolate work, personal, research files
- **0% data leaves your machine.**

## Why?
I have thousands of PDFs and notes. I don't remember filenames. I remember "that invoice about server costs" or "the rust code where I fixed the memory leak".
Typical regex search fails here. Vector search doesn't.

## Tech Stack
- **Frontend**: React + TypeScript (Vite)
- **Backend**: Rust (Tauri 2)
- **Vectors**: [LanceDB](https://lancedb.com/) (embedded, no docker junk)
- **Embedding**: `Multilingual-E5-Base` (768-dim, ~280MB)
- **Reranker**: `JINA Reranker v2` (multilingual)
- **OCR**: Windows.Media.Ocr (built-in, zero install)
- **UI**: Windows 11 Fluent / Mica (looks native)

## How to run
You need Rust and Node installed.

```bash
npm install
npm run tauri dev        # dev (downloads model on first run, ~280mb)
npm run tauri build      # release
```

## Usage
- **Alt + Space**: Toggle the search bar (global shortcut)
- **Ctrl + O**: Index a new folder
- **Esc**: Clear search or hide window

## Configuration
`%AppData%\com.recall-lite.app\config.json`

```json
{
  "embedding_model": "MultilingualE5Base",
  "containers": { ... },
  "active_container": "Default"
}
```
*Supported models: AllMiniLML6V2, MultilingualE5Small, MultilingualE5Base*

## Performance
- Tested on 10k+ files
- Indexing is CPU-bound (first run takes a few minutes)
- Search is <50ms (release build)

## License
MIT. Do whatever.
