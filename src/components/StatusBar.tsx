import { Loader2 } from "lucide-react";
import type { IndexingProgress } from "../types";
import { useLocale } from "../i18n";

interface StatusBarProps {
    status: string;
    isIndexing: boolean;
    indexProgress: IndexingProgress | null;
    activeContainer: string;
    indexedFolderCount: number;
    resultCount: number;
}

export default function StatusBar({
    status, isIndexing, indexProgress, activeContainer, indexedFolderCount, resultCount,
}: StatusBarProps) {
    const { t } = useLocale();

    const pct = indexProgress && indexProgress.total > 0
        ? Math.round((indexProgress.current / indexProgress.total) * 100)
        : 0;

    return (
        <div className="status-bar shrink-0 px-6 flex flex-col justify-center select-none text-[--color-text-secondary]">
            {isIndexing && indexProgress && indexProgress.total > 0 && (
                <div className="progress-bar-track">
                    <div
                        className="progress-bar-fill"
                        style={{ width: `${pct}%` }}
                    />
                </div>
            )}
            <div className="flex items-center justify-between text-[11px] h-8">
                <div className="flex items-center gap-3 overflow-hidden">
                    <span className="font-semibold text-[--color-fill-accent-default] opacity-90">{activeContainer}</span>
                    <span className="w-px h-3 bg-[--color-stroke-divider-default]"></span>
                    {status ? (
                        <span className="flex items-center gap-2 truncate">
                            <Loader2 className="animate-spin" size={10} />
                            {indexProgress && indexProgress.total > 0
                                ? `${pct}% · ${status}`
                                : status
                            }
                        </span>
                    ) : (
                        <span>{t("status_indexed_folders", { count: String(indexedFolderCount), results: String(resultCount) })}</span>
                    )}
                </div>
                <div className="flex items-center gap-4 opacity-80 px-2">
                    <span className="flex items-center gap-1.5"><span className="font-mono text-[10px] bg-[--color-control-fill-secondary] px-1.5 py-0.5 rounded">↑↓</span> {t("results_navigate")}</span>
                    <span className="flex items-center gap-1.5"><span className="font-mono text-[10px] bg-[--color-control-fill-secondary] px-1.5 py-0.5 rounded">↵</span> {t("results_open")}</span>
                </div>
            </div>
        </div>
    );
}
