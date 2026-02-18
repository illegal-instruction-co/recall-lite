# Recall Lite

semantic search tool for humans and AI agents. you type meaning, it finds files. nothing leaves your machine.

**windows 10+ only** for now. uses UWP OCR and mica backdrop. first run downloads ~2GB of models, needs internet once.

<p align="center">
  <img src="https://github.com/user-attachments/assets/b11b0fc1-3a35-4854-8703-ff98f286a430" width="700" />
</p>

## what it does

- indexes 120+ file types (code, docs, images, configs, whatever)
- OCR on images via windows built-in engine
- reads EXIF → reverse geocodes GPS to city names. search "photos from istanbul" and it works
- EXIF dates → human words. "summer morning" finds a photo from july at 8am
- hybrid search: vector + full-text + JINA cross-encoder reranker
- smart chunking per language (rust at `fn`/`struct`, python at `def`/`class`, etc)
- semantic containers for isolation (work/personal/research)
- MCP server for AI agents. [details →](MCP.md) · [agent instructions →](AGENT.md)

## run it
```bash
npm install
npm run tauri dev        # downloads ~2GB models on first run
npm run tauri build      # release build, use this for real speed
```

`Alt+Space` to toggle. config & docs → [CONFIG.md](CONFIG.md)

RAM usage peaks during initial indexing — this is expected. once indexing completes, it drops and stays stable.

## agentic benchmark

same 5 tasks, same codebase. grep vs recall-lite MCP:

| task | grep | recall-lite |
|------|------|-------------|
| "find where GPS coords become city names" | grep "GPS" → 0. grep "geocode" → found file, need to open. **3 steps** | **1 step** |
| "find the quality filter threshold" | grep "threshold" → 0 (code says `>= 25.0`). **failed** | **1 step** |
| "find dedup logic for best chunk per file" | grep "dedup" → 0. grep "best" → noise. **3-5 steps** | **1 step** |
| "find config migration handling" | grep "legacy" → wrong file. **wrong answer** | **1 step** |
| "find embedding batch size constant" | grep "batch_size" → 0 (it's `EMBED_BATCH_SIZE`). **failed** | **1 step** |

**grep needs the exact keyword. recall-lite needs the idea.**

agents using recall-lite are expected to use 5-10x fewer tokens and complete tasks significantly faster. fewer search attempts, fewer wrong files opened, fewer round-trips. the benchmark above shows 1 step vs 3-5 — that's both speed and cost.

<p align="center">
  <img src="https://github.com/user-attachments/assets/35ce8fb8-b24f-4a45-86fb-80e0eae9baa3" width="300" />
</p>

## stack
rust (tauri 2), react/ts, [lancedb](https://lancedb.com/), Multilingual-E5-Base, JINA Reranker v2, rayon

## roadmap
[ROADMAP.md](ROADMAP.md)

## star history

<a href="https://star-history.com/#illegal-instruction-co/recall-lite&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=illegal-instruction-co/recall-lite&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=illegal-instruction-co/recall-lite&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=illegal-instruction-co/recall-lite&type=Date" />
 </picture>
</a>

## contributors

<a href="https://github.com/illegal-instruction-co/recall-lite/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=illegal-instruction-co/recall-lite" />
</a>

## contributing

[CONTRIBUTING.md](CONTRIBUTING.md)

## license
MIT
