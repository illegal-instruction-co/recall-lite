import { useRef, useEffect, useState, useMemo } from "react";
import type { ListImperativeAPI } from "react-window";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { useModal, ModalProvider } from "./Modal";
import { useLocale } from "./i18n";
import Sidebar from "./components/Sidebar";
import SearchBar from "./components/SearchBar";
import ResultsList from "./components/ResultsList";
import StatusBar from "./components/StatusBar";
import TitleBar from "./components/TitleBar";
import Settings from "./components/Settings";
import type { SearchResult, IndexingProgress, ContainerItem } from "./types";
import logoSrc from "./assets/rememex.png";
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
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [hotkey, setHotkey] = useState("Alt + Space");
  const [annotations, setAnnotations] = useState<{ id: string; path: string; note: string; source: string; created_at: number }[]>([]);
  const [selectedAnnotationId, setSelectedAnnotationId] = useState<string | null>(null);
  const modal = useModal();
  const { t } = useLocale();

  const searchInputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<ListImperativeAPI>(null);
  const isFirstRunRef = useRef(false);

  useEffect(() => {
    fetchContainers();
    invoke<{ first_run: boolean; provider_type: string; hotkey: string }>("get_config").then((c) => {
      setHotkey(c.hotkey);
      if (c.first_run) {
        isFirstRunRef.current = true;
        setSettingsOpen(true);
        invoke("update_config", { updates: { first_run: false } }).catch(() => { });
      }
    }).catch(() => { });
  }, []);

  async function fetchAnnotations() {
    try {
      const list = await invoke<{ id: string; path: string; note: string; source: string; created_at: number }[]>("get_annotations", { path: null });
      annotationCountRef.current = list.length;
      setAnnotations(list);
    } catch {
      annotationCountRef.current = 0;
      setAnnotations([]);
    }
  }

  const annotationCountRef = useRef(0);

  useEffect(() => {
    fetchAnnotations();
    const interval = setInterval(async () => {
      try {
        const list = await invoke<{ id: string; path: string; note: string; source: string; created_at: number }[]>("get_annotations", { path: null });
        if (list.length !== annotationCountRef.current) {
          annotationCountRef.current = list.length;
          setAnnotations(list);
        }
      } catch { /* ignore */ }
    }, 5000);
    return () => clearInterval(interval);
  }, [activeContainer]);

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
    const step1 = await modal.prompt({
      title: t("dialog_new_container"),
      icon: "info",
      fields: [
        { key: "name", label: t("dialog_field_name"), placeholder: t("dialog_field_name_placeholder") },
        { key: "description", label: t("dialog_field_description"), placeholder: t("dialog_field_description_placeholder") },
        {
          key: "provider_type", label: "Provider", type: "select" as const,
          defaultValue: "local",
          options: [
            { value: "local", label: "Local (on-device)" },
            { value: "remote", label: "Remote (API)" },
          ],
        },
      ],
      confirmText: t("dialog_next"),
    });

    if (!step1.confirmed || !step1.values?.name?.trim()) return;

    const providerType = step1.values.provider_type || "local";
    let embeddingModel = "MultilingualE5Base";
    let remoteEndpoint = "";
    let remoteApiKey = "";
    let remoteModel = "";
    let remoteDimensions = 1024;

    if (providerType === "local") {
      const step2 = await modal.prompt({
        title: "Embedding Model",
        icon: "info",
        fields: [
          {
            key: "embedding_model", label: "Model", type: "select" as const,
            defaultValue: "MultilingualE5Base",
            options: [
              { value: "AllMiniLML6V2", label: "MiniLM L6 v2 (Fast)" },
              { value: "MultilingualE5Small", label: "Multilingual E5 Small" },
              { value: "MultilingualE5Base", label: "Multilingual E5 Base" },
            ],
          },
        ],
        confirmText: t("dialog_create"),
      });
      if (!step2.confirmed) return;
      embeddingModel = step2.values?.embedding_model || "MultilingualE5Base";
    } else {
      const step2 = await modal.prompt({
        title: "Remote Provider",
        icon: "info",
        fields: [
          { key: "endpoint", label: "Endpoint", placeholder: "https://api.openai.com/v1/embeddings" },
          { key: "api_key", label: "API Key", type: "password" as const, placeholder: "sk-..." },
          { key: "model", label: "Model Name", placeholder: "text-embedding-3-small" },
          { key: "dimensions", label: "Dimensions", type: "number" as const, defaultValue: "1024", placeholder: "1024" },
        ],
        confirmText: t("dialog_create"),
      });

      if (!step2.confirmed || !step2.values?.endpoint?.trim()) return;

      remoteEndpoint = step2.values.endpoint.trim();
      remoteApiKey = (step2.values.api_key || "").trim();
      remoteModel = (step2.values.model || "").trim();
      remoteDimensions = Number.parseInt(step2.values.dimensions || "1024", 10) || 1024;
    }

    try {
      await invoke("create_container", {
        name: step1.values.name.trim(),
        description: (step1.values.description || "").trim(),
        providerType,
        embeddingModel,
        remoteEndpoint: remoteEndpoint || null,
        remoteApiKey: remoteApiKey || null,
        remoteModel: remoteModel || null,
        remoteDimensions: remoteDimensions || null,
      });
      await fetchContainers();
      await handleSwitchContainer(step1.values.name.trim());
    } catch (e) {
      await modal.confirm({ title: "Error", message: String(e), icon: "warning", confirmText: t("modal_ok") });
    }
  }

  async function handleDeleteContainer() {
    if (activeContainer === "Default") return;

    const result = await modal.confirm({
      title: t("dialog_delete_title"),
      message: t("dialog_delete_message", { name: activeContainer }),
      icon: "warning",
      confirmText: t("dialog_delete_confirm"),
      confirmVariant: "danger",
    });

    if (result.confirmed) {
      try {
        await invoke("delete_container", { name: activeContainer });
        await fetchContainers();
        setResults([]);
      } catch (e) {
        await modal.confirm({ title: "Error", message: String(e), icon: "warning", confirmText: t("modal_ok") });
      }
    }
  }

  async function handleSwitchContainer(name: string) {
    if (name === activeContainer) return;
    setActiveContainer(name);
    setResults([]);
    setQuery("");
    setSelectedAnnotationId(null);
    setStatus(t("status_switched", { name }));
    searchInputRef.current?.focus();
    try {
      await invoke("set_active_container", { name });
    } catch (e) {
      console.error(e);
    }
  }

  useEffect(() => {
    searchInputRef.current?.focus();
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

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
          title: t("dialog_clear_title"),
          message: t("dialog_clear_message", { name: activeContainer }),
          icon: "warning",
          confirmText: t("dialog_clear_confirm"),
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
      setStatus(t("status_done", { message: event.payload }));
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
      setStatus(t("status_model_error", { error: event.payload }));
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
          setStatus(t("status_rebuild_needed"));
        } else {
          setStatus(msg);
        }
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [query, activeContainer]);

  async function handleResetIndex() {
    try {
      setStatus(t("status_clearing"));
      setIsIndexing(true);
      await invoke("reset_index");
      setResults([]);
      setStatus(t("status_cleared"));
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
      title: t("dialog_rebuild_title"),
      message: t("dialog_rebuild_message", { count: String(activeInfo.indexed_paths.length), name: activeContainer }),
      icon: "info",
      confirmText: t("dialog_rebuild_confirm"),
    });

    if (!result.confirmed) return;

    try {
      setStatus(t("status_rebuilding"));
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
        title: t("index_folder_title", { container: activeContainer }),
      });
      if (selected) {
        setStatus(t("status_starting"));
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

  async function handleAnnotate(path: string) {
    const result = await modal.prompt({
      title: t("annotation_add"),
      icon: "info",
      fields: [
        { key: "note", label: t("annotation_placeholder"), placeholder: t("annotation_placeholder") },
      ],
      confirmText: t("annotation_save"),
    });

    if (!result.confirmed || !result.values?.note?.trim()) return;

    try {
      await invoke("add_annotation", { path, note: result.values.note.trim() });
      setStatus(t("annotation_saved"));
      fetchAnnotations();
    } catch (e) {
      await modal.confirm({ title: "Error", message: String(e), icon: "warning", confirmText: t("modal_ok") });
    }
  }

  async function handleDeleteAnnotation(id: string) {
    try {
      await invoke("delete_annotation", { annotationId: id });
      fetchAnnotations();
    } catch (e) {
      console.error(e);
    }
  }

  const activeInfo = containers.find(c => c.name === activeContainer);
  const selectedAnnotation = useMemo(() => annotations.find(a => a.id === selectedAnnotationId) ?? null, [annotations, selectedAnnotationId]);

  return (
    <>
      <div className="app-container" style={{ '--logo-url': `url(${logoSrc})` } as React.CSSProperties}>
        <TitleBar />
        <Sidebar
          containers={containers}
          activeContainer={activeContainer}
          isIndexing={isIndexing}
          sidebarOpen={sidebarOpen}
          annotations={annotations}
          onToggleSidebar={() => setSidebarOpen(prev => !prev)}
          onSwitchContainer={handleSwitchContainer}
          onCreateContainer={handleCreateContainer}
          onDeleteContainer={handleDeleteContainer}
          onReindexAll={handleReindexAll}
          onOpenSettings={() => setSettingsOpen(true)}
          onDeleteAnnotation={handleDeleteAnnotation}
          onSelectAnnotation={(id) => { setSelectedAnnotationId(id); setQuery(""); }}
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
          {selectedAnnotation ? (
            <div className="annotation-detail-view">
              <div className="annotation-detail-header">
                <button type="button" className="annotation-detail-close" onClick={() => setSelectedAnnotationId(null)}>✕</button>
                <span className="annotation-detail-path" title={selectedAnnotation.path}>{selectedAnnotation.path}</span>
                <span className={`annotation-source-badge ${selectedAnnotation.source}`}>
                  {selectedAnnotation.source === 'agent' ? '🤖' : '👤'}
                </span>
              </div>
              <div className="annotation-detail-meta">
                {new Date(selectedAnnotation.created_at * 1000).toLocaleString()}
              </div>
              <div className="annotation-detail-note">{selectedAnnotation.note}</div>
              <div className="annotation-detail-actions">
                <button type="button" className="annotation-detail-open" onClick={() => handleOpenFile(selectedAnnotation.path)}>
                  Open File
                </button>
                <button type="button" className="annotation-detail-delete" onClick={() => { handleDeleteAnnotation(selectedAnnotation.id); setSelectedAnnotationId(null); }}>
                  Delete
                </button>
              </div>
            </div>
          ) : (
            <ResultsList
              results={results}
              selectedIndex={selectedIndex}
              setSelectedIndex={setSelectedIndex}
              activeContainer={activeContainer}
              query={query}
              onOpenFile={(p) => { handleOpenFile(p).catch(() => { }); }}
              onAnnotate={(p) => { handleAnnotate(p).catch(() => { }); }}
              listRef={listRef}
              hotkey={hotkey}
            />
          )}
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
      <Settings open={settingsOpen} onClose={() => {
        setSettingsOpen(false);
        if (isFirstRunRef.current) {
          isFirstRunRef.current = false;
          invoke<{ provider_type: string; remote_endpoint: string; remote_api_key: string; remote_model: string; remote_dimensions: number; embedding_model: string }>("get_config").then((c) => {
            invoke("update_config", {
              updates: {
                provider_type: c.provider_type,
                remote_endpoint: c.remote_endpoint,
                remote_api_key: c.remote_api_key,
                remote_model: c.remote_model,
                remote_dimensions: c.remote_dimensions,
                embedding_model: c.embedding_model,
              }
            }).catch(() => { });
          }).catch(() => { });
        }
      }} />
    </>
  );
}

export default App;
