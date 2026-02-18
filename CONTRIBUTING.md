# contributing

thanks for wanting to help. here's how.

## found a bug?

[open an issue](https://github.com/illegal-instruction-co/recall-lite/issues/new?template=bug_report.md). include what you did, what happened, what you expected. logs help: `%AppData%\com.recall-lite.app\recall.log`

## want a feature?

[open a feature request](https://github.com/illegal-instruction-co/recall-lite/issues/new?template=feature_request.md). explain the use case, not just the solution.

## want to write code?

1. fork the repo
2. create a branch (`git checkout -b fix/thing-that-broke`)
3. make your changes
4. test locally:
   ```bash
   cd src-tauri
   cargo clippy --all-targets
   cargo test
   cd ..
   npm run tauri dev
   ```
5. open a PR against `main`

### what we care about

- **no comments in code.** the code should explain itself. if it can't, refactor it
- **keep PRs small.** one fix per PR. one feature per PR. don't bundle
- **test your changes.** if you add a feature, make sure it works. if you fix a bug, make sure it stays fixed
- **match the existing style.** look at the code around your change and follow the patterns

### project structure

```
src/                    # react frontend (tsx)
src-tauri/src/          # rust backend
  ├── indexer/          # file indexing, chunking, embedding
  ├── commands.rs       # tauri commands (frontend ↔ backend)
  ├── config.rs         # configuration management
  ├── watcher.rs        # file system watcher
  ├── lib.rs            # app setup, search pipeline
  └── bin/mcp.rs        # MCP server binary
```

### dev setup

prerequisites: rust toolchain, node.js 18+, windows 10+

```bash
git clone https://github.com/illegal-instruction-co/recall-lite.git
cd recall-lite
npm install
npm run tauri dev    # first run downloads ~2GB of models
```

## labels

- `good first issue` — small, well-defined tasks for newcomers
- `help wanted` — we'd appreciate help on these
- `bug` — something's broken
- `enhancement` — new feature or improvement

## questions?

open a [discussion](https://github.com/illegal-instruction-co/recall-lite/discussions) or an issue. don't be shy.
