import { useRef, useEffect, useState } from "react";
import { List, type ListImperativeAPI } from "react-window";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  FileText, FileCode, FileJson, Image as ImageIcon, File,
  Loader2, FolderPlus, Search, Box, Plus, Trash2, FolderOpen,
  PanelLeftClose, PanelLeftOpen
} from "lucide-react";
import { useModal, ModalProvider } from "./Modal";
import "./App.css";

interface SearchResult {
  path: string;
  snippet: string;
  score: number;
}

function getScoreColor(score: number): string {
  if (score > 80) return "bg-green-500/10 text-green-400";
  if (score > 50) return "bg-yellow-500/10 text-yellow-400";
  return "bg-gray-500/10 text-gray-500";
}

function getFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function getFileIcon(path: string) {
  const ext = path.split(".").pop()?.toLowerCase() || "";
  const props = { className: "w-5 h-5 text-gray-400" };
  switch (ext) {
    case "pdf": return <FileText {...props} />;
    case "txt": case "md": return <FileText {...props} />;
    case "rs": case "ts": case "js": case "py": return <FileCode {...props} />;
    case "json": return <FileJson {...props} />;
    case "png": case "jpg": return <ImageIcon {...props} />;
    default: return <File {...props} />;
  }
}

interface RowData {
  results: SearchResult[];
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  handleOpenFile: (path: string) => void;
}

const Row = ({ index, style, results, selectedIndex, setSelectedIndex, handleOpenFile }: { index: number; style: React.CSSProperties } & RowData) => {
  const result = results[index];
  const isSelected = index === selectedIndex;

  return (
    <div style={style} className="px-3">
      <button
        type="button"
        key={result.path}
        data-active={isSelected}
        onClick={() => { setSelectedIndex(index); handleOpenFile(result.path); }}
        className="result-item w-full text-left flex items-start gap-3 cursor-default outline-none select-none group h-full"
      >
        <div className="pt-0.5 shrink-0 opacity-80 group-hover:opacity-100 transition-opacity">
          {getFileIcon(result.path)}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex justify-between items-baseline gap-2">
            <h4 className="text-body truncate leading-tight">
              {getFileName(result.path)}
            </h4>
            <span className={`text-[10px] font-sans px-1.5 rounded-full shrink-0 ${getScoreColor(result.score)} bg-opacity-20`}>
              {Math.round(result.score)}%
            </span>
          </div>
          <div className="truncate text-caption mt-0.5 opacity-60">
            {result.snippet || <span className="italic opacity-50">No preview available</span>}
          </div>
          <div className="truncate text-[10px] opacity-40 mt-0.5 font-mono">
            {result.path}
          </div>
        </div>
      </button>
    </div>
  );
};

interface ContainerItem {
  name: string;
  description: string;
  indexed_paths: string[];
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [status, setStatus] = useState("");
  const [isIndexing, setIsIndexing] = useState(false);

  const [containers, setContainers] = useState<ContainerItem[]>([]);
  const [activeContainer, setActiveContainer] = useState("Default");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const modal = useModal();

  const searchInputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<ListImperativeAPI>(null);
  const [listDims, setListDims] = useState({ width: 0, height: 0 });

  useEffect(() => {
    if (!resultsRef.current) return;
    const observer = new ResizeObserver(entries => {
      const { width, height } = entries[0].contentRect;
      setListDims({ width, height });
    });
    observer.observe(resultsRef.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    fetchContainers();
  }, []);

  async function fetchContainers() {
    try {
      const [list, active] = await invoke<[ContainerItem[], string]>("get_containers");
      setContainers(list);
      setActiveContainer(active);
    } catch (e) {
      console.error("Failed to fetch containers", e);
    }
  }

  async function handleCreateContainer() {
    const result = await modal.prompt({
      title: "New Container",
      icon: "info",
      fields: [
        { key: "name", label: "Name", placeholder: "Work, Gaming, Research..." },
        { key: "description", label: "Description (AI Context)", placeholder: "accounting files for acme corp" },
      ],
      confirmText: "Create",
    });

    if (!result.confirmed || !result.values?.name?.trim()) return;

    try {
      await invoke("create_container", {
        name: result.values.name.trim(),
        description: (result.values.description || "").trim(),
      });
      await fetchContainers();
    } catch (e) {
      await modal.confirm({ title: "Error", message: String(e), icon: "warning", confirmText: "OK" });
    }
  }

  async function handleDeleteContainer() {
    if (activeContainer === "Default") return;

    const result = await modal.confirm({
      title: "Delete Container",
      message: `Are you sure you want to delete '${activeContainer}'? All indexed data will be lost forever.`,
      icon: "warning",
      confirmText: "Delete",
      confirmVariant: "danger",
    });

    if (result.confirmed) {
      try {
        await invoke("delete_container", { name: activeContainer });
        await fetchContainers();
        setResults([]);
      } catch (e) {
        await modal.confirm({ title: "Error", message: String(e), icon: "warning", confirmText: "OK" });
      }
    }
  }

  async function handleSwitchContainer(name: string) {
    if (name === activeContainer) return;
    try {
      await invoke("set_active_container", { name });
      setActiveContainer(name);
      setResults([]); // Clear results from previous container
      setQuery("");
      setStatus(`Switched to ${name}`);
      searchInputRef.current?.focus();
    } catch (e) {
      console.error(e);
    }
  }




  useEffect(() => {
    if (listRef.current && results.length > 0) {
      listRef.current.scrollToRow({ index: selectedIndex });
    }
  }, [selectedIndex, results]);




  useEffect(() => {
    searchInputRef.current?.focus();
    const handleKeyDown = (e: KeyboardEvent) => {
      // ... existing key handlers ...
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex(prev => Math.min(prev + 1, results.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex(prev => Math.max(prev - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (results[selectedIndex]) {
          handleOpenFile(results[selectedIndex].path);
        }
      } else if ((e.ctrlKey || e.metaKey) && e.key === "o") {
        e.preventDefault();
        handlePickFolder();
      } else if (e.shiftKey && e.key === "Delete") {
        e.preventDefault();
        // Context-aware delete
        if (confirm(`Clear index for '${activeContainer}'?`)) {
          handleResetIndex();
        }
      } else if (e.key === "Escape") {
        if (query) setQuery("");
      }
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, [results, selectedIndex, query, activeContainer]);

  useEffect(() => {
    const unlistenProgress = listen<string>("indexing-progress", (event) => {
      setStatus(`Indexing: ${getFileName(event.payload)}`);
      setIsIndexing(true);
    });

    const unlistenModelLoaded = listen("model-loaded", () => {
      setStatus("");
      setIsIndexing(false);
    });

    const unlistenModelError = listen<string>("model-load-error", (event) => {
      setStatus(`Model Error: ${event.payload}`);
      setIsIndexing(false);
    });

    return () => {
      unlistenProgress.then((f) => f());
      unlistenModelLoaded.then((f) => f());
      unlistenModelError.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const res = await invoke<SearchResult[]>("search", { query });
        setResults(res);
        setSelectedIndex(0);
      } catch (err) {
        setStatus(String(err));
      }
    }, 150);
    return () => clearTimeout(timer);
  }, [query, activeContainer]);

  async function handleResetIndex() {
    try {
      setStatus("Clearing index...");
      setIsIndexing(true);
      await invoke("reset_index");
      setResults([]);
      setStatus("Index cleared.");
      setIsIndexing(false);
    } catch (err) {
      setStatus(String(err));
      setIsIndexing(false);
    }
  }

  async function handlePickFolder() {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: `Index folder into '${activeContainer}'`,
      });
      if (selected) {
        setStatus("Starting indexing...");
        setIsIndexing(true);
        const msg = await invoke<string>("index_folder", { dir: selected });
        setStatus(msg);
        setIsIndexing(false);
        await fetchContainers();
      }
    } catch (err) {
      setStatus(String(err));
      setIsIndexing(false);
    }
  }

  async function handleOpenFile(path: string) {
    try {
      await openPath(path);
    } catch (e) {
      console.error("Failed to open file:", path, e);
      setStatus(`Failed to open: ${String(e)}`);
    }
  }

  // Helpers


  return (
    <>
      <div className="app-container">

        {/* Sidebar */}
        <div className={`sidebar ${sidebarOpen ? '' : 'collapsed'}`}>
          <div className="sidebar-header">
            <button className="sidebar-btn" onClick={() => setSidebarOpen(prev => !prev)} title={sidebarOpen ? 'Collapse sidebar' : 'Expand sidebar'}>
              {sidebarOpen ? <PanelLeftClose size={14} /> : <PanelLeftOpen size={14} />}
            </button>
            {sidebarOpen && (
              <>
                <span className="sidebar-title flex-1">Containers</span>
                <button className="sidebar-btn" onClick={handleCreateContainer} title="Create Container">
                  <Plus size={14} />
                </button>
              </>
            )}
          </div>
          {sidebarOpen && (
            <>
              <div className="container-list">
                {containers.map(c => (
                  <div key={c.name} className="container-item-wrapper">
                    <button
                      type="button"
                      className={`container-item w-full text-left ${activeContainer === c.name ? 'active' : ''}`}
                      onClick={() => handleSwitchContainer(c.name)}
                    >
                      <Box size={14} className="icon" />
                      <div className="flex-1 min-w-0">
                        <span className="truncate block">{c.name}</span>
                        {c.description && (
                          <span className="truncate block text-[10px] opacity-40 mt-0.5">{c.description}</span>
                        )}
                      </div>
                    </button>
                    {activeContainer === c.name && c.indexed_paths.length > 0 && (
                      <div className="indexed-paths">
                        {c.indexed_paths.map(p => (
                          <div key={p} className="indexed-path-item">
                            <FolderOpen size={10} className="shrink-0 opacity-40" />
                            <span className="truncate">{p}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                ))}
              </div>
              {activeContainer !== "Default" && (
                <button
                  className="flex items-center justify-center gap-2 p-2 text-[11px] text-red-400/80 hover:text-red-400 hover:bg-red-500/10 rounded transition-colors"
                  onClick={handleDeleteContainer}
                >
                  <Trash2 size={12} /> Delete Container
                </button>
              )}
            </>
          )}
        </div>

        {/* Main Content */}
        <div className="main-content">
          {/* Search Header */}
          <div className="search-wrapper shrink-0">
            <div className="relative">
              <Search className="absolute left-4 top-1/2 -translate-y-1/2 text-[--color-text-tertiary] pointer-events-none" size={18} />
              <input
                ref={searchInputRef}
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={`Search in ${activeContainer}...`}
                className="search-input"
                autoFocus
              />
              <button
                onClick={handlePickFolder}
                className="absolute right-4 top-1/2 -translate-y-1/2 p-2 rounded-md hover:bg-[--color-control-fill-secondary] text-[--color-text-secondary] transition-colors"
                title={`Index Folder into ${activeContainer} (Ctrl+O)`}
              >
                {isIndexing ? <Loader2 className="animate-spin" size={18} /> : <FolderPlus size={18} />}
              </button>
            </div>
          </div>

          {/* Results Area */}
          <div className="flex-1 overflow-hidden min-h-0 mt-2 pb-3" ref={resultsRef}>
            {results.length === 0 && !query && (
              <div className="h-full flex flex-col items-center justify-center text-[--color-text-muted] select-none opacity-60">
                <Box size={40} className="mb-4 opacity-40 text-[--color-fill-accent-default]" strokeWidth={1} />
                <p className="text-body font-medium">{activeContainer}</p>
                <p className="text-caption mt-1">Container Active</p>

                <div className="mt-8 flex flex-col gap-2 items-center">
                  <p className="text-[10px] uppercase tracking-wider opacity-60">Shortcuts</p>
                  <div className="flex gap-4 opacity-50 text-xs font-mono">
                    <span>Ctrl + O : Index</span>
                    <span>Alt + Space : Toggle</span>
                  </div>
                </div>
              </div>
            )}

            {results.length === 0 && query && (
              <div className="h-full flex flex-col items-center justify-center text-[--color-text-muted] select-none opacity-60">
                <p className="text-body font-medium">No results found</p>
                <p className="text-caption mt-1">in {activeContainer}</p>
              </div>
            )}

            {results.length > 0 && listDims.height > 0 && (
              <List<RowData>
                listRef={listRef}
                style={{ width: listDims.width, height: listDims.height }}
                rowCount={results.length}
                rowHeight={78}
                rowProps={{ results, selectedIndex, setSelectedIndex, handleOpenFile: (p: string) => { handleOpenFile(p).catch(() => { }); } }}
                className="result-list-virtualized"
                rowComponent={Row}
              />
            )}
          </div>

          {/* Status Bar Footer */}
          <div className="status-bar shrink-0 h-8 px-6 flex items-center justify-between text-[11px] select-none text-[--color-text-secondary]">
            <div className="flex items-center gap-3 overflow-hidden">
              <span className="font-semibold text-[--color-fill-accent-default] opacity-90">{activeContainer}</span>
              <span className="w-px h-3 bg-[--color-stroke-divider-default]"></span>
              {status ? (
                <span className="flex items-center gap-2 truncate"><Loader2 className="animate-spin" size={10} /> {status}</span>
              ) : (
                <span>{results.length} items</span>
              )}
            </div>
            <div className="flex items-center gap-4 opacity-80 px-2">
              <span className="flex items-center gap-1.5"><span className="font-mono text-[10px] bg-[--color-control-fill-secondary] px-1.5 py-0.5 rounded">↑↓</span> to navigate</span>
              <span className="flex items-center gap-1.5"><span className="font-mono text-[10px] bg-[--color-control-fill-secondary] px-1.5 py-0.5 rounded">↵</span> to open</span>
            </div>
          </div>
        </div>
      </div>
      <ModalProvider />
    </>
  );
}

export default App;
