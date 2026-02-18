# Rust crate versions and API audit, early 2026

**All nine crate families are actively maintained and broadly compatible**, but three critical friction points demand attention: fastembed's exact pin on `ort =2.0.0-rc.11` constrains your ONNX dependency graph, winit 0.30's focus-stealing behavior has no clean cross-platform fix, and lancedb's rapid pre-1.0 churn means expect API breakage between minor bumps. Below is a crate-by-crate breakdown with exact versions from crates.io, API signatures, and every known incompatibility.

---

## eframe 0.33.3 and the ViewportBuilder surface

**egui and eframe are both at 0.33.3** (released 2025-12-11), requiring Rust **1.88.0+**. eframe pins **winit ^0.30.12**, **raw-window-handle ^0.6.2**, and optionally **wgpu ^27.0.1** (glow 0.16 is the default renderer). The winit 0.30 migration landed in egui 0.29.0, replacing the closure-based event loop with the `ApplicationHandler` trait.

`ViewportBuilder` exposes **29 `Option<T>` fields**. The three fields relevant to overlay-style windows:

```rust
ViewportBuilder::default()
    .with_transparent(true)          // transparent: Option<bool>
    .with_always_on_top()            // sets window_level = WindowLevel::AlwaysOnTop
    .with_taskbar(false)             // taskbar: Option<bool> — hides from taskbar
    .with_decorations(false)
    .with_inner_size([320.0, 240.0])
```

Transparency requires **two** steps: set `with_transparent(true)` on the builder *and* override `App::clear_color()` to return `egui::Rgba::TRANSPARENT`. On macOS, also call `with_has_shadow(false)` to eliminate ghosting. A known issue (GitHub #4451) reports transparency producing a **black window** with the glow backend on Windows — switching to the wgpu backend resolves it.

**Breaking changes across recent releases** worth tracking: egui **0.32.0** rewrote menus/popups (old `Memory::popup` deprecated) and introduced Atoms for buttons. egui **0.33.0** replaced `Context::on_begin_pass`/`on_end_pass` with a Plugin trait, added safe areas (`content_rect` replaces `screen_rect`), and introduced a **30-second deadlock panic** on `egui::Mutex` in debug builds — switch to `std::sync::Mutex` or `parking_lot` for long-held locks.

---

## window-vibrancy 0.7.1 is compatible with eframe 0.33

**Version 0.7.1** (2025-11-12) depends on **raw-window-handle ^0.6**, making it directly compatible with eframe 0.33.x without adapter crates. The breaking v0.4→v0.5 migration switched from `HasRawWindowHandle` to `HasWindowHandle`; all current APIs use the new trait:

```rust
// apply_mica: Windows 11 only
pub fn apply_mica(window: impl HasWindowHandle, dark: Option<bool>) -> Result<(), Error>

// apply_acrylic: Windows 10 v1809+  
pub fn apply_acrylic(window: impl HasWindowHandle, color: Option<(u8,u8,u8,u8)>) -> Result<(), Error>
```

eframe's `CreationContext` implements `HasWindowHandle`, so you pass `&cc` directly:

```rust
eframe::run_native("App", options, Box::new(|cc| {
    #[cfg(target_os = "windows")]
    window_vibrancy::apply_mica(&cc, None).ok();
    Ok(Box::new(MyApp::default()))
}));
```

**Version 0.7.0** added macOS **Liquid Glass** support (`apply_liquid_glass`/`clear_liquid_glass`). Other Windows 11 APIs include `apply_tabbed` (Mica Tabbed) and their corresponding `clear_*` functions. A **known performance issue**: `apply_acrylic` causes window lag on resize/drag on Windows 10 v1903+ and Windows 11 build 22000+. This is a Microsoft compositor limitation, not a crate bug. Linux remains **completely unsupported** — vibrancy depends on the compositor.

---

## global-hotkey 0.7.0 and tray-icon 0.21.3 work standalone

Both crates are from `tauri-apps` but have **zero Tauri dependency**. They share an identical event pattern using `crossbeam-channel`:

| Crate | Version | Event receiver |
|-------|---------|---------------|
| global-hotkey | **0.7.0** (2025-05-07) | `GlobalHotKeyEvent::receiver().try_recv()` |
| tray-icon | **0.21.3** (2026-01-03) | `TrayIconEvent::receiver().try_recv()` + `MenuEvent::receiver().try_recv()` |

**global-hotkey integration with eframe**: create `GlobalHotKeyManager` before calling `eframe::run_native`, then poll `GlobalHotKeyEvent::receiver().try_recv()` inside your `App::update()` method. The manager must live on the same thread as the native event loop (mandatory on macOS main thread). Version 0.7.0 switched the Linux backend from `x11-dl` to `x11rb`. **No Wayland support** — X11 only on Linux.

**tray-icon** re-exports `muda ^0.17` as `tray_icon::menu::*`, so no separate muda dependency is needed. The `TrayIconBuilder` API is straightforward:

```rust
let tray = TrayIconBuilder::new()
    .with_menu(Box::new(menu))
    .with_tooltip("My App")
    .with_icon(icon)
    .build()?;
```

On **Linux with eframe**, a special pattern is required: spawn a separate thread running `gtk::init()` + `gtk::main()` to host the tray icon, because eframe/winit doesn't use GTK. On Windows and macOS, create the tray icon directly in the `CreationContext` setup closure. Version 0.21.3 fixed Windows tray re-registration when the taskbar is recreated (`TaskbarCreated` message handling).

---

## The fastembed + ort version lock is the hardest constraint

**fastembed 5.8.1** (~January 2026) pins **`ort = "=2.0.0-rc.11"`** with an *exact version* requirement. **ort 2.0.0-rc.11** (2026-01-07) wraps ONNX Runtime 1.23 and remains in release-candidate status after two years of RC iterations. All ort 1.x versions have been **yanked from crates.io**.

The practical impact: if your project depends on both `fastembed` and `ort` directly, you **must** use `ort = "=2.0.0-rc.11"` — any other version triggers a Cargo resolution failure. The `ort-sys` crate uses `links = "onnxruntime"`, enforcing a single ort version across the entire dependency tree. Each fastembed point release has bumped this pin (rc.9 → rc.10 → rc.11), so **expect lockfile churn**.

**fastembed API essentials**:

```rust
// Embedding
let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))?;
let embeddings: Vec<Vec<f32>> = model.embed(documents, None)?;

// Reranking
let reranker = TextRerank::try_new(RerankInitOptions::new(RerankerModel::BGERerankerBase))?;
let ranked = reranker.rerank("query", documents, true, None)?;
```

ONNX Runtime bundling is handled by the `ort-download-binaries` feature (enabled by default in fastembed) — it downloads pre-built static libraries at build time via `ort-sys`. No external ONNX Runtime install needed. For dynamic linking, use the `ort-load-dynamic` feature instead.

**ort 2.x static linking**: controlled via `ort-sys` feature flags — `download-binaries` for build-time static linking, `load-dynamic` for runtime `.dll`/`.so` loading, `copy-dylibs` to place shared libraries in the output directory. Major v1→v2 changes include module reorganization (`Session` → `ort::session::Session`), removal of the global environment, and ndarray bumped from 0.15 to 0.17.

---

## lancedb 0.26.2 supports hybrid search natively

**lancedb 0.26.2** (2026-02-09) runs fully **in-process** with no server. It pins `lance = 2.0.0` and uses `arrow ^57.2` and `datafusion ^51.0`. Connect to a local path:

```rust
let db = lancedb::connect("data/sample-lancedb").execute().await?;
```

**Hybrid search** combines vector similarity and BM25 full-text search. Create both index types, then chain methods on the query builder:

```rust
// Vector search
table.query().nearest_to(&query_vec)?.execute().await?;

// Full-text search (Tantivy-based BM25)
table.query().full_text_search(FullTextSearchQuery::new("hello world".into())).execute().await?;
```

The Rust API exposes normalization modes (`"rank"` or `"score"`) for hybrid result merging. The Python SDK is further ahead with explicit `query_type="hybrid"` and built-in rerankers (LinearCombination, RRF). A notable build caveat: the transitive dependency `lzma-sys` requires `lzma-sys = { version = "*", features = ["static"] }` in your Cargo.toml to link correctly. The crate is **pre-1.0** with a rapid release cadence — expect breaking changes between minor versions.

---

## xcap 0.8.2 replaces the deprecated screenshots crate

**xcap 0.8.2** (2026-02-10) is the active successor to the `screenshots` crate (0.8.10, deprecated, unmaintained since early 2024). xcap supports **Windows, macOS, and Linux** (both X11 via xcb and Wayland via libwayshot/pipewire):

```rust
let monitors = Monitor::all()?;
let image: RgbaImage = monitor.capture_image()?;    // full screen
let region = monitor.capture_region(x, y, w, h)?;   // partial

let windows = Window::all()?;
let img = window.capture_image()?;                   // single window
```

All property accessors return `Result<T>` in 0.8.x (a breaking change from pre-0.5 versions that returned values directly). The crate depends on `windows 0.61` on Windows and `image 0.25`. Note: **docs.rs fails to build** xcap 0.8.2 due to Linux build dependencies, so refer to the GitHub repo for documentation.

---

## windows 0.62.2 for WinRT OCR via RecognizeAsync

**windows 0.62.2** (2025-10-06) is the latest stable release of Microsoft's official Rust projection. For OCR, you **must** use the `windows` crate (not `windows-sys`, which lacks WinRT support entirely). Required feature flags:

```toml
[dependencies.windows]
version = ">=0.59, <=0.62"
features = ["Media_Ocr", "Graphics_Imaging", "Win32_System_WinRT"]
```

The `RecognizeAsync` pattern:

```rust
let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
let result = engine.RecognizeAsync(&software_bitmap)?.await?;
for line in result.Lines()? {
    let text = line.Text()?;
    for word in line.Words()? {
        let rect = word.BoundingRect()?;
    }
}
```

`IAsyncOperation` implements `Future`, so `RecognizeAsync` integrates directly with `tokio` or any async runtime. Converting raw pixels to `SoftwareBitmap` requires `IMemoryBufferByteAccess` from `Win32_System_WinRT` to write into the bitmap's locked buffer. Microsoft recommends **version ranges** (`>=0.59, <=0.62`) in Cargo.toml to prevent duplicate `windows` crate versions in your dependency graph — important since xcap already pulls in `windows 0.61`.

---

## Cross-crate incompatibilities and workarounds

**eframe + window-vibrancy**: Fully compatible as of eframe 0.33.3 and window-vibrancy 0.7.1. Both use `raw-window-handle ^0.6`. The only friction is the transparency backend: use **wgpu** (not glow) on Windows to avoid the black-window bug (#4451).

**fastembed + ort**: Compatible *only* at the exact pinned version. To use both, explicitly declare `ort = "=2.0.0-rc.11"` with any additional features you need. Cargo will union feature flags. Do not attempt to use a different ort version.

**winit 0.30 focus stealing on Windows**: `SetForegroundWindow` is restricted by the OS — only the foreground process's child processes can claim focus. `Window::focus_window()` fails silently for minimized or invisible windows, flashing the taskbar icon instead. `ViewportBuilder::with_active(false)` is **unreliable across platforms** — macOS still steals focus (winit #3072), X11 has no unfocused-map primitive (winit #1160). The practical workaround for overlay apps: accept focus steal on creation, then use `WS_EX_NOACTIVATE` extended style via raw Win32 interop (`SetWindowLongPtrW`) post-creation on Windows to prevent subsequent focus grabs.

**windows crate version overlap**: xcap 0.8.2 depends on `windows 0.61`; your OCR code uses `windows 0.62`. Use Microsoft's recommended range syntax (`>=0.59, <=0.62`) to let Cargo deduplicate.

| Crate | Latest version | raw-window-handle | Key constraint |
|-------|---------------|-------------------|----------------|
| eframe | **0.33.3** | ^0.6.2 | winit ^0.30.12, MSRV 1.88 |
| window-vibrancy | **0.7.1** | ^0.6 | No Linux support |
| global-hotkey | **0.7.0** | — | No Wayland; X11 only |
| tray-icon | **0.21.3** | — | Needs GTK thread on Linux+eframe |
| lancedb | **0.26.2** | — | Pre-1.0; needs `lzma-sys` static |
| fastembed | **5.8.1** | — | Pins `ort =2.0.0-rc.11` exactly |
| ort | **2.0.0-rc.11** | — | All 1.x yanked; still RC |
| xcap | **0.8.2** | — | Depends on windows 0.61 |
| windows | **0.62.2** | — | Use range `>=0.59, <=0.62` |

## Conclusion

The stack is viable but requires precise version management. The **single highest-risk dependency** is the `fastembed → ort` exact pin — each fastembed release can break your lockfile, and ort has been in RC for over two years with no stable 2.0 date announced. For production stability, consider vendoring the ort version or pinning fastembed tightly. The eframe + window-vibrancy pairing is now clean after the shared migration to `raw-window-handle 0.6`, but stick with the wgpu backend on Windows. For focus-stealing workarounds, raw Win32 `WS_EX_NOACTIVATE` via the `windows` crate remains the most reliable approach — winit has no plans to expose this natively.