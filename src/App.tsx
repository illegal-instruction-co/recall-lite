import { useRef, useEffect, useState } from "react";
import type { ListImperativeAPI } from "react-window";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { useModal, ModalProvider } from "./Modal";
import Sidebar from "./components/Sidebar";
import SearchBar from "./components/SearchBar";
import ResultsList from "./components/ResultsList";
import StatusBar from "./components/StatusBar";
import type { SearchResult, IndexingProgress, ContainerItem } from "./types";
import "./App.css";

function getFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [status, setStatus] = useState("");
  const [isIndexing, setIsIndexing] = useState(false);
  const [indexProgress, setIndexProgress] = useState<IndexingProgress | null>(null);

  const [containers, setContainers] = useState<ContainerItem[]>([]);
  const [activeContainer, setActiveContainer] = useState("Default");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const modal = useModal();

  const searchInputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<ListImperativeAPI>(null);

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
      setResults([]);
      setQuery("");
      setStatus(`Switched to ${name}`);
      searchInputRef.current?.focus();
    } catch (e) {
      console.error(e);
    }
  }

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
        modal.confirm({
          title: "Clear Index",
          message: `Clear index for '${activeContainer}'?`,
          icon: "warning",
          confirmText: "Clear",
          confirmVariant: "danger",
        }).then((result) => {
          if (result.confirmed) handleResetIndex();
        });
      } else if (e.key === "Escape") {
        if (query) setQuery("");
      }
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, [results, selectedIndex, query, activeContainer]);

  useEffect(() => {
    const unlistenProgress = listen<IndexingProgress>("indexing-progress", (event) => {
      setStatus(`Indexing: ${getFileName(event.payload.path)}`);
      setIndexProgress(event.payload);
      setIsIndexing(true);
    });

    const unlistenComplete = listen<string>("indexing-complete", (event) => {
      setStatus(`Done — ${event.payload}`);
      setIsIndexing(false);
      setIndexProgress(null);
      setTimeout(() => setStatus(""), 5000);
    });

    const unlistenModelLoaded = listen("model-loaded", () => {
      setStatus("");
      setIsIndexing(false);
      setIndexProgress(null);
    });

    const unlistenModelError = listen<string>("model-load-error", (event) => {
      setStatus(`Model Error: ${event.payload}`);
      setIsIndexing(false);
      setIndexProgress(null);
    });

    return () => {
      unlistenProgress.then((f) => f());
      unlistenComplete.then((f) => f());
      unlistenModelLoaded.then((f) => f());
      unlistenModelError.then((f) => f());
    };
  }, []);

  const searchGenRef = useRef(0);

  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    const gen = ++searchGenRef.current;
    const timer = setTimeout(async () => {
      try {
        const res = await invoke<SearchResult[]>("search", { query });
        if (searchGenRef.current !== gen) return;
        setResults(res);
        setSelectedIndex(0);
      } catch (err) {
        if (searchGenRef.current !== gen) return;
        const msg = String(err);
        if (msg.includes("rebuild") || msg.includes("Model changed")) {
          setStatus("Index needs rebuild — click Rebuild Index");
        } else {
          setStatus(msg);
        }
      }
    }, 300);
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

  async function handleReindexAll() {
    const activeInfo = containers.find(c => c.name === activeContainer);
    if (!activeInfo || activeInfo.indexed_paths.length === 0) return;

    const result = await modal.confirm({
      title: "Rebuild Index",
      message: `This will re-index all ${activeInfo.indexed_paths.length} folder(s) in '${activeContainer}' with improved embeddings. This may take a moment.`,
      icon: "info",
      confirmText: "Rebuild",
    });

    if (!result.confirmed) return;

    try {
      setStatus("Rebuilding index...");
      setIsIndexing(true);
      setResults([]);
      const msg = await invoke<string>("reindex_all");
      setStatus(msg);
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

  const activeInfo = containers.find(c => c.name === activeContainer);

  return (
    <>
      <div className="app-container">
        <Sidebar
          containers={containers}
          activeContainer={activeContainer}
          isIndexing={isIndexing}
          sidebarOpen={sidebarOpen}
          onToggleSidebar={() => setSidebarOpen(prev => !prev)}
          onSwitchContainer={handleSwitchContainer}
          onCreateContainer={handleCreateContainer}
          onDeleteContainer={handleDeleteContainer}
          onReindexAll={handleReindexAll}
        />
        <div className="main-content">
          <SearchBar
            query={query}
            onQueryChange={setQuery}
            activeContainer={activeContainer}
            isIndexing={isIndexing}
            onPickFolder={handlePickFolder}
            inputRef={searchInputRef}
          />
          <ResultsList
            results={results}
            selectedIndex={selectedIndex}
            setSelectedIndex={setSelectedIndex}
            activeContainer={activeContainer}
            query={query}
            onOpenFile={(p) => { handleOpenFile(p).catch(() => { }); }}
            listRef={listRef}
          />
          <StatusBar
            status={status}
            isIndexing={isIndexing}
            indexProgress={indexProgress}
            activeContainer={activeContainer}
            indexedFolderCount={activeInfo?.indexed_paths.length || 0}
            resultCount={results.length}
          />
        </div>
      </div>
      <ModalProvider />
    </>
  );
}

export default App;
