# Recall Lite (local-mind)

Windows search sucks. Copilot is creepy. I needed something that finds my sh*t without sending my screen to the cloud.

So I built this.

![SetupContainer](https://github.com/user-attachments/assets/75573638-2fda-4a68-bf61-30aaf5d2ad67)

![Search](https://github.com/user-attachments/assets/98407df3-1984-4f3d-9cd5-21cbfdc4cb85)

Spotlight can't find what you mean. This can.

## what it does
local semantic search. you type meaning, it finds files. runs in your system tray, never phones home.

- indexes 50+ file types (pdf, images, code, configs, markdown, csv, logs, dotfiles, whatever)
- OCR on images via windows built-in engine. zero install
- reads EXIF from photos --> camera, lens, aperture, ISO, focal length, gps coordinates
- **reverse geocodes GPS to actual city names**. so yeah you can search "photos from istanbul" and it works
- dates from EXIF get expanded to human words --> day names, months, time of day, season, in both english and turkish. search "summer morning" and find a photo from july
- hybrid search: vector similarity + full-text + JINA cross-encoder reranker
- query expansion strips stop words (EN + TR), generates keyword variants
- smart chunking --> splits rust at `fn`/`struct`, python at `def`/`class`, markdown at headers, yaml at top-level keys. chunk sizes tuned per format
- semantic containers --> isolate work/personal/research. each gets its own DB table. delete one, zero remnants
- parallel file extraction via rayon --> all your CPU cores working at once
- i18n support --> JSON locale files, auto-detects system language. drop a json file to add your language

## stack
- rust (tauri 2) + react/ts (vite)
- [lancedb](https://lancedb.com/) --> embedded vector db, no docker, no server
- `Multilingual-E5-Base` for embeddings (768-dim, ~280MB)
- `JINA Reranker v2` for cross-encoding (multilingual)
- `reverse_geocoder` crate for offline GPS lookups
- `rayon` for parallel file processing
- windows mica/acrylic backdrop. looks native

## run it
```bash
npm install
npm run tauri dev        # downloads models on first run (~800mb)
npm run tauri build      # release. use this for real speed
```

## shortcuts
- `Alt + Space` --> toggle window (global)
- `Ctrl + O` --> pick folder to index
- `up/down` --> navigate, `Enter` --> open file
- `Esc` --> clear
- `Shift + Delete` --> nuke current index

## config
`%AppData%\com.recall-lite.app\config.json`

models: AllMiniLML6V2, MultilingualE5Small, MultilingualE5Base

logs: `%AppData%\com.recall-lite.app\recall.log`

## misc
- incremental indexing (mtime check, skips unchanged)
- streams embeddings in 256-chunk batches, constant memory
- search works during indexing --> model lock per batch, not per session
- reranker runs on blocking threadpool so it doesnt choke async
- auto-migrates old configs, retries model load 3x, cleans up legacy cache
- release builds only. debug is 10x slower thats normal

## roadmap
see [ROADMAP.md](ROADMAP.md).

## license
MIT. do whatever.
