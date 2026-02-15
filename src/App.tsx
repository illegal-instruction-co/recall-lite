import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  FileText, FileCode, FileJson, Image as ImageIcon, File,
  Loader2, Command, FolderPlus, Search
} from "lucide-react";
import "./App.css";

interface SearchResult {
  path: string;
  snippet: string;
  score: number;
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [status, setStatus] = useState("");
  const [isIndexing, setIsIndexing] = useState(false);

  const searchInputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    searchInputRef.current?.focus();
    const handleKeyDown = (e: KeyboardEvent) => {
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
        if (confirm("Clear index? This cannot be undone.")) {
          handleResetIndex();
        }
      } else if (e.key === "Escape") {
        if (query) setQuery("");
      }
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, [results, selectedIndex, query]);

  useEffect(() => {
    if (resultsRef.current && results.length > 0) {
      const activeEl = resultsRef.current.children[selectedIndex] as HTMLElement;
      if (activeEl) {
        activeEl.scrollIntoView({ block: "nearest", behavior: "smooth" });
      }
    }
  }, [selectedIndex, results]);

  useEffect(() => {
    const unlisten = listen<string>("indexing-progress", (event) => {
      setStatus(`Indexing: ${getFileName(event.payload)}`);
      setIsIndexing(true);
    });
    return () => {
      unlisten.then((f) => f());
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
  }, [query]);

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
        title: "Select folder to index",
      });
      if (selected) {
        setStatus("Starting indexing...");
        setIsIndexing(true);
        const msg = await invoke<string>("index_folder", { dir: selected });
        setStatus(msg);
        setIsIndexing(false);
      }
    } catch (err) {
      setStatus(String(err));
      setIsIndexing(false);
    }
  }

  async function handleOpenFile(path: string) {
    try {
      await open(path);
    } catch (e) {
      console.error(e);
      setStatus("Failed to open file");
    }
  }

  // getFileName moved to outer scope

  // getFileIcon moved to outer scope
  // getScoreColor moved to outer scope

  return (
    <div className="app-container flex-1 flex flex-col min-h-0 bg-transparent">

      {/* Search Header */}
      <div className="search-wrapper shrink-0">
        <div className="relative">
          <Search className="absolute left-4 top-1/2 -translate-y-1/2 text-[--color-text-tertiary] pointer-events-none" size={18} />
          <input
            ref={searchInputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search files..."
            className="search-input"
            autoFocus
          />
          <button
            onClick={handlePickFolder}
            className="absolute right-4 top-1/2 -translate-y-1/2 p-2 rounded-md hover:bg-[--color-control-fill-secondary] text-[--color-text-secondary] transition-colors"
            title="Index Folder (Ctrl+O)"
          >
            {isIndexing ? <Loader2 className="animate-spin" size={18} /> : <FolderPlus size={18} />}
          </button>
        </div>
      </div>

      {/* Results Area */}
      <div className="flex-1 overflow-y-auto min-h-0 mt-2 px-3 pb-3" ref={resultsRef}>
        {results.length === 0 && !query && (
          <div className="h-full flex flex-col items-center justify-center text-[--color-text-muted] select-none opacity-60">
            <Command size={48} strokeWidth={1} className="mb-4 opacity-50" />
            <p className="text-body font-medium">Type to search</p>
            <p className="text-caption mt-1">or use Ctrl+O to index a folder</p>
          </div>
        )}

        {results.length === 0 && query && (
          <div className="h-full flex flex-col items-center justify-center text-[--color-text-muted] select-none opacity-60">
            <p className="text-body font-medium">No results found</p>
            <p className="text-caption mt-1">Try a different keyword</p>
          </div>
        )}

        <div className="result-list">
          {results.map((result, i) => (
            <button
              type="button"
              key={result.path}
              data-active={i === selectedIndex}
              onClick={() => { setSelectedIndex(i); handleOpenFile(result.path); }}
              className="result-item w-full text-left flex items-start gap-3 cursor-default outline-none select-none group"
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
          ))}
        </div>
      </div>

      {/* Status Bar Footer */}
      <div className="status-bar shrink-0 h-8 px-4 flex items-center justify-between text-[11px] select-none text-[--color-text-secondary]">
        <div className="flex items-center gap-3 overflow-hidden">
          {status ? (
            <span className="flex items-center gap-2 truncate"><Loader2 className="animate-spin" size={10} /> {status}</span>
          ) : (
            <span>{results.length} items</span>
          )}
        </div>
        <div className="flex items-center gap-4 opacity-80">
          <span className="flex items-center gap-1.5"><span className="font-mono text-[10px] bg-[--color-control-fill-secondary] px-1 rounded">↑↓</span> to navigate</span>
          <span className="flex items-center gap-1.5"><span className="font-mono text-[10px] bg-[--color-control-fill-secondary] px-1 rounded">↵</span> to open</span>
        </div>
      </div>
    </div>
  );
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

function getScoreColor(score: number): string {
  if (score > 80) return "bg-green-500/10 text-green-400";
  if (score > 50) return "bg-yellow-500/10 text-yellow-400";
  return "bg-gray-500/10 text-gray-500";
}

export default App;
