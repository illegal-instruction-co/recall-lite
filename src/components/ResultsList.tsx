import { useRef, useEffect, useState } from "react";
import { List, type ListImperativeAPI } from "react-window";
import {
    FileText, FileCode, FileJson, Image as ImageIcon, File, Box,
} from "lucide-react";
import type { SearchResult } from "../types";
import { useLocale } from "../i18n";

function getScoreColor(score: number): string {
    if (score > 80) return "bg-green-500/10 text-green-400";
    if (score > 65) return "bg-yellow-500/10 text-yellow-400";
    return "bg-orange-500/10 text-orange-400";
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
    noPreviewText: string;
}

const Row = ({ index, style, results, selectedIndex, setSelectedIndex, handleOpenFile, noPreviewText }: { index: number; style: React.CSSProperties } & RowData) => {
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
                        {result.snippet || <span className="italic opacity-50">{noPreviewText}</span>}
                    </div>
                    <div className="truncate text-[10px] opacity-40 mt-0.5 font-mono">
                        {result.path}
                    </div>
                </div>
            </button>
        </div>
    );
};

interface ResultsListProps {
    results: SearchResult[];
    selectedIndex: number;
    setSelectedIndex: (index: number) => void;
    activeContainer: string;
    query: string;
    onOpenFile: (path: string) => void;
    listRef: React.RefObject<ListImperativeAPI | null>;
    hotkey: string;
}

export default function ResultsList({
    results, selectedIndex, setSelectedIndex, activeContainer, query, onOpenFile, listRef, hotkey,
}: Readonly<ResultsListProps>) {
    const { t } = useLocale();
    const containerRef = useRef<HTMLDivElement>(null);
    const [dims, setDims] = useState({ width: 0, height: 0 });

    useEffect(() => {
        if (!containerRef.current) return;
        const observer = new ResizeObserver(entries => {
            const { width, height } = entries[0].contentRect;
            setDims({ width, height });
        });
        observer.observe(containerRef.current);
        return () => observer.disconnect();
    }, []);

    useEffect(() => {
        if (listRef.current && results.length > 0) {
            listRef.current.scrollToRow({ index: selectedIndex });
        }
    }, [selectedIndex, results, listRef]);

    return (
        <div className="flex-1 overflow-hidden min-h-0 mt-2 pb-3" ref={containerRef}>
            {results.length === 0 && !query && (
                <div className="h-full flex flex-col items-center justify-center text-[--color-text-muted] select-none opacity-60">
                    <Box size={40} className="mb-4 opacity-40 text-[--color-fill-accent-default]" strokeWidth={1} />
                    <p className="text-body font-medium">{activeContainer}</p>
                    <p className="text-caption mt-1">{t("results_container_active")}</p>

                    <div className="mt-8 flex flex-col gap-2 items-center">
                        <p className="text-[10px] uppercase tracking-wider opacity-60">{t("results_shortcuts")}</p>
                        <div className="flex gap-4 opacity-50 text-xs font-mono">
                            <span>{t("results_shortcut_index")}</span>
                            <span>{hotkey} : {t("results_shortcut_toggle").split(" : ").pop()}</span>
                        </div>
                    </div>
                </div>
            )}

            {results.length === 0 && query && (
                <div className="h-full flex flex-col items-center justify-center text-[--color-text-muted] select-none opacity-60">
                    <p className="text-body font-medium">{t("results_no_results")}</p>
                    <p className="text-caption mt-1">{t("results_in_container", { container: activeContainer })}</p>
                </div>
            )}

            {results.length > 0 && dims.height > 0 && (
                <List<RowData>
                    listRef={listRef}
                    style={{ width: dims.width, height: dims.height }}
                    rowCount={results.length}
                    rowHeight={78}
                    rowProps={{ results, selectedIndex, setSelectedIndex, handleOpenFile: (p: string) => { onOpenFile(p); }, noPreviewText: t("results_no_preview") }}
                    className="result-list-virtualized"
                    rowComponent={Row}
                />
            )}
        </div>
    );
}
