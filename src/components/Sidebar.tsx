import {
    Box, Plus, Trash2, FolderOpen, Folder, RefreshCw,
    PanelLeftClose, PanelLeftOpen, Globe,
} from "lucide-react";
import { SettingsButton } from "./Settings";
import type { ContainerItem } from "../types";
import { useLocale } from "../i18n";

const localeLabels: Record<string, string> = {
    en: "English",
    tr: "Türkçe",
};

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
    onOpenSettings: () => void;
}

export default function Sidebar({
    containers, activeContainer, isIndexing, sidebarOpen,
    onToggleSidebar, onSwitchContainer, onCreateContainer,
    onDeleteContainer, onReindexAll, onOpenSettings,
}: Readonly<SidebarProps>) {
    const { t, locale, setLocale, availableLocales } = useLocale();

    function cycleLocale() {
        const idx = availableLocales.indexOf(locale);
        setLocale(availableLocales[(idx + 1) % availableLocales.length]);
    }

    return (
        <div className={`sidebar ${sidebarOpen ? '' : 'collapsed'}`}>
            <div className="sidebar-header">
                <button className="sidebar-btn" onClick={onToggleSidebar} title={sidebarOpen ? t('sidebar_collapse') : t('sidebar_expand')}>
                    {sidebarOpen ? <PanelLeftClose size={14} /> : <PanelLeftOpen size={14} />}
                </button>
                {sidebarOpen && (
                    <>
                        <span className="sidebar-title sidebar-title-flex">{t('sidebar_title')}</span>
                        <SettingsButton onClick={onOpenSettings} />
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
                                    className={`container-item container-item-full ${activeContainer === c.name ? 'active' : ''} ${isIndexing ? 'disabled' : ''}`}
                                    onClick={() => !isIndexing && onSwitchContainer(c.name)}
                                    disabled={isIndexing}
                                >
                                    <Box size={14} className="icon" />
                                    <div className="container-item-content">
                                        <span className="container-item-name">{c.name}</span>
                                        {c.description && (
                                            <span className="container-item-desc">{c.description}</span>
                                        )}
                                        {c.provider_label && (
                                            <span className="container-item-desc" style={{ opacity: 0.3, fontSize: '9px' }}>{c.provider_label}</span>
                                        )}
                                    </div>
                                </button>
                                {activeContainer === c.name && (
                                    <div className="indexed-paths-section">
                                        <div className="indexed-paths-header">
                                            <Folder size={10} className="indexed-paths-icon" />
                                            <span>{t('sidebar_indexed_folders')}</span>
                                        </div>
                                        {c.indexed_paths.length > 0 ? (
                                            <>
                                                <div className="indexed-paths">
                                                    {c.indexed_paths.map(p => (
                                                        <div key={p} className="indexed-path-item" title={p}>
                                                            <FolderOpen size={10} className="indexed-path-icon" />
                                                            <span className="indexed-path-text">{p.split(/[\\/]/).slice(-2).join('/')}</span>
                                                        </div>
                                                    ))}
                                                </div>
                                                <button
                                                    className="reindex-btn"
                                                    onClick={onReindexAll}
                                                    disabled={isIndexing}
                                                    title={t('sidebar_rebuild_tooltip')}
                                                >
                                                    <RefreshCw size={10} className={isIndexing ? 'reindex-spin' : ''} />
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
                        <button className="sidebar-delete-btn" onClick={onDeleteContainer}>
                            <Trash2 size={12} /> {t('sidebar_delete')}
                        </button>
                    )}
                    <button className="locale-switcher" onClick={cycleLocale} title={localeLabels[locale] ?? locale}>
                        <Globe size={12} />
                        <span>{locale.toUpperCase()}</span>
                    </button>
                </>
            )}
        </div>
    );
}
