import { useState, useEffect } from "react";
import { Settings as SettingsIcon, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useLocale } from "../i18n";
import GeneralSettings from "./settings/GeneralSettings";
import IndexingSettings from "./settings/IndexingSettings";
import "./Settings.css";

interface AppConfig {
    always_on_top: boolean;
    launch_at_startup: boolean;
    hotkey: string;
    use_git_history: boolean;
    embedding_model: string;
    chunk_size: number | null;
    chunk_overlap: number | null;
    extra_extensions: string[];
    excluded_extensions: string[];
    provider_type: string;
    remote_endpoint: string;
    remote_api_key: string;
    remote_model: string;
    remote_dimensions: number;
    first_run: boolean;
    use_reranker: boolean;
}

interface SettingsProps {
    open: boolean;
    onClose: () => void;
}

export function SettingsButton({ onClick }: Readonly<{ onClick: () => void }>) {
    const { t } = useLocale();
    return (
        <button className="sidebar-btn" onClick={onClick} title={t("settings_title")}>
            <SettingsIcon size={14} />
        </button>
    );
}

export default function Settings({ open, onClose }: Readonly<SettingsProps>) {
    const { t } = useLocale();
    const [config, setConfig] = useState<AppConfig | null>(null);
    const [hotkeyDraft, setHotkeyDraft] = useState("");
    const [hotkeyDirty, setHotkeyDirty] = useState(false);
    const [extraExtDraft, setExtraExtDraft] = useState("");
    const [excludedExtDraft, setExcludedExtDraft] = useState("");

    useEffect(() => {
        if (open) {
            invoke<AppConfig>("get_config").then((c) => {
                setConfig(c);
                setHotkeyDraft(c.hotkey);
                setHotkeyDirty(false);
                setExtraExtDraft(c.extra_extensions.join(", "));
                setExcludedExtDraft(c.excluded_extensions.join(", "));
            });
        }
    }, [open]);

    async function updateField(updates: Record<string, unknown>) {
        await invoke("update_config", { updates });
        const updated = await invoke<AppConfig>("get_config");
        setConfig(updated);
        setHotkeyDraft(updated.hotkey);
    }

    if (!open || !config) return null;

    return (
        <div className="settings-overlay" role="none" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
            <div className="settings-panel">
                <div className="settings-header">
                    <h2 className="settings-title">{t("settings_title")}</h2>
                    <button type="button" className="settings-close" onClick={onClose}>
                        <X size={14} />
                    </button>
                </div>

                <div className="settings-body">
                    <GeneralSettings
                        config={config}
                        hotkeyDraft={hotkeyDraft}
                        hotkeyDirty={hotkeyDirty}
                        onHotkeyChange={setHotkeyDraft}
                        updateField={updateField}
                        setHotkeyDirty={setHotkeyDirty}
                    />

                    <div className="settings-group">
                        <div className="settings-section-title">{t("settings_section_indexing")}</div>

                        <IndexingSettings
                            config={config}
                            extraExtDraft={extraExtDraft}
                            excludedExtDraft={excludedExtDraft}
                            setExtraExtDraft={setExtraExtDraft}
                            setExcludedExtDraft={setExcludedExtDraft}
                            updateField={updateField}
                        />
                    </div>
                </div>
            </div>
        </div>
    );
}
