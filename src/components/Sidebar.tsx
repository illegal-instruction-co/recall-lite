import {
    Box, Plus, Trash2, FolderOpen, Folder, RefreshCw,
    PanelLeftClose, PanelLeftOpen,
} from "lucide-react";
import type { ContainerItem } from "../types";
import { useLocale } from "../i18n";

interface SidebarProps {
    containers: ContainerItem[];
    activeContainer: string;
    isIndexing: boolean;
    sidebarOpen: boolean;
    onToggleSidebar: () => void;
    onSwitchContainer: (name: string) => void;
    onCreateContainer: () => void;
    onDeleteContainer: () => void;
    onReindexAll: () => void;
}

export default function Sidebar({
    containers, activeContainer, isIndexing, sidebarOpen,
    onToggleSidebar, onSwitchContainer, onCreateContainer,
    onDeleteContainer, onReindexAll,
}: SidebarProps) {
    const { t } = useLocale();

    return (
        <div className={`sidebar ${sidebarOpen ? '' : 'collapsed'}`}>
            <div className="sidebar-header">
                <button className="sidebar-btn" onClick={onToggleSidebar} title={sidebarOpen ? t('sidebar_collapse') : t('sidebar_expand')}>
                    {sidebarOpen ? <PanelLeftClose size={14} /> : <PanelLeftOpen size={14} />}
                </button>
                {sidebarOpen && (
                    <>
                        <span className="sidebar-title flex-1">{t('sidebar_title')}</span>
                        <button className="sidebar-btn" onClick={onCreateContainer} title={t('sidebar_create')}>
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
                                    onClick={() => onSwitchContainer(c.name)}
                                >
                                    <Box size={14} className="icon" />
                                    <div className="flex-1 min-w-0">
                                        <span className="truncate block">{c.name}</span>
                                        {c.description && (
                                            <span className="truncate block text-[10px] opacity-40 mt-0.5">{c.description}</span>
                                        )}
                                    </div>
                                </button>
                                {activeContainer === c.name && (
                                    <div className="indexed-paths-section">
                                        <div className="indexed-paths-header">
                                            <Folder size={10} className="opacity-40" />
                                            <span>{t('sidebar_indexed_folders')}</span>
                                        </div>
                                        {c.indexed_paths.length > 0 ? (
                                            <>
                                                <div className="indexed-paths">
                                                    {c.indexed_paths.map(p => (
                                                        <div key={p} className="indexed-path-item" title={p}>
                                                            <FolderOpen size={10} className="shrink-0 opacity-50" />
                                                            <span className="truncate">{p.split(/[\\/]/).slice(-2).join('/')}</span>
                                                        </div>
                                                    ))}
                                                </div>
                                                <button
                                                    className="reindex-btn"
                                                    onClick={onReindexAll}
                                                    disabled={isIndexing}
                                                    title={t('sidebar_rebuild_tooltip')}
                                                >
                                                    <RefreshCw size={10} className={isIndexing ? 'animate-spin' : ''} />
                                                    <span>{t('sidebar_rebuild')}</span>
                                                </button>
                                            </>
                                        ) : (
                                            <div className="indexed-paths-empty">
                                                {t('sidebar_no_folders')}
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                    {activeContainer !== "Default" && (
                        <button
                            className="flex items-center justify-center gap-2 p-2 text-[11px] text-red-400/80 hover:text-red-400 hover:bg-red-500/10 rounded transition-colors"
                            onClick={onDeleteContainer}
                        >
                            <Trash2 size={12} /> {t('sidebar_delete')}
                        </button>
                    )}
                </>
            )}
        </div>
    );
}
