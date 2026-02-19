import { Brain, Cloud, Server, Link, Key, Hash } from "lucide-react";
import { useLocale } from "../../i18n";
import { SettingsRow } from "./SettingsRow";
import "./ProviderSettings.css";

interface ProviderConfig {
    provider_type: string;
    embedding_model: string;
}

const modelLabels: Record<string, string> = {
    AllMiniLML6V2: "MiniLM L6 v2 (Fast)",
    MultilingualE5Small: "Multilingual E5 Small",
    MultilingualE5Base: "Multilingual E5 Base",
};

interface Props {
    config: ProviderConfig;
    remoteEndpointDraft: string;
    remoteApiKeyDraft: string;
    remoteModelDraft: string;
    remoteDimsDraft: string;
    providerChanged: boolean;
    setRemoteEndpointDraft: (v: string) => void;
    setRemoteApiKeyDraft: (v: string) => void;
    setRemoteModelDraft: (v: string) => void;
    setRemoteDimsDraft: (v: string) => void;
    setProviderChanged: (v: boolean) => void;
    updateField: (updates: Record<string, unknown>) => Promise<void>;
}

export default function ProviderSettings({
    config, remoteEndpointDraft, remoteApiKeyDraft, remoteModelDraft, remoteDimsDraft,
    providerChanged, setRemoteEndpointDraft, setRemoteApiKeyDraft, setRemoteModelDraft,
    setRemoteDimsDraft, setProviderChanged, updateField,
}: Readonly<Props>) {
    const { t } = useLocale();

    return (
        <>
            <SettingsRow
                icon={<Brain size={14} />}
                label={t("settings_provider_type")}
                desc={t("settings_provider_type_desc")}
                control={
                    <div className="settings-provider-toggle">
                        <button
                            type="button"
                            className={`provider-btn ${config.provider_type === "local" ? "active" : ""}`}
                            onClick={() => {
                                setProviderChanged(true);
                                updateField({ provider_type: "local" });
                            }}
                        >
                            <Server size={12} />
                            {t("settings_provider_local")}
                        </button>
                        <button
                            type="button"
                            className={`provider-btn ${config.provider_type === "remote" ? "active" : ""}`}
                            onClick={() => {
                                setProviderChanged(true);
                                updateField({
                                    provider_type: "remote",
                                    remote_endpoint: remoteEndpointDraft,
                                    remote_api_key: remoteApiKeyDraft,
                                    remote_model: remoteModelDraft,
                                    remote_dimensions: Number.parseInt(remoteDimsDraft, 10) || 1024,
                                });
                            }}
                        >
                            <Cloud size={12} />
                            {t("settings_provider_remote")}
                        </button>
                    </div>
                }
            />

            {config.provider_type === "local" && (
                <SettingsRow
                    icon={<Brain size={14} />}
                    label={t("settings_embedding_model")}
                    desc={t("settings_embedding_model_desc")}
                    control={
                        <select
                            className="settings-select"
                            value={config.embedding_model}
                            aria-label={t("settings_embedding_model")}
                            onChange={(e) => updateField({ embedding_model: e.target.value })}
                        >
                            {Object.entries(modelLabels).map(([key, label]) => (
                                <option key={key} value={key}>{label}</option>
                            ))}
                        </select>
                    }
                />
            )}
            {config.provider_type === "local" && config.embedding_model !== "MultilingualE5Base" && (
                <span className="settings-row-note">{t("settings_restart_reindex")}</span>
            )}

            {config.provider_type === "remote" && (
                <div className="settings-remote-fields">
                    <SettingsRow
                        icon={<Link size={14} />}
                        label={t("settings_remote_endpoint")}
                        desc={t("settings_remote_endpoint_desc")}
                        control={
                            <input
                                type="text"
                                className="settings-ext-input"
                                value={remoteEndpointDraft}
                                placeholder="http://localhost:11434/v1/embeddings"
                                spellCheck={false}
                                onChange={(e) => setRemoteEndpointDraft(e.target.value)}
                                onBlur={() => updateField({ remote_endpoint: remoteEndpointDraft })}
                                onKeyDown={(e) => { if (e.key === "Enter") updateField({ remote_endpoint: remoteEndpointDraft }); }}
                            />
                        }
                    />
                    <SettingsRow
                        icon={<Key size={14} />}
                        label={t("settings_remote_api_key")}
                        desc={t("settings_remote_api_key_desc")}
                        control={
                            <input
                                type="password"
                                className="settings-ext-input"
                                value={remoteApiKeyDraft}
                                placeholder="sk-..."
                                spellCheck={false}
                                onChange={(e) => setRemoteApiKeyDraft(e.target.value)}
                                onBlur={() => updateField({ remote_api_key: remoteApiKeyDraft })}
                                onKeyDown={(e) => { if (e.key === "Enter") updateField({ remote_api_key: remoteApiKeyDraft }); }}
                            />
                        }
                    />
                    <SettingsRow
                        icon={<Brain size={14} />}
                        label={t("settings_remote_model")}
                        desc={t("settings_remote_model_desc")}
                        control={
                            <input
                                type="text"
                                className="settings-ext-input"
                                value={remoteModelDraft}
                                placeholder="mxbai-embed-large"
                                spellCheck={false}
                                onChange={(e) => setRemoteModelDraft(e.target.value)}
                                onBlur={() => updateField({ remote_model: remoteModelDraft })}
                                onKeyDown={(e) => { if (e.key === "Enter") updateField({ remote_model: remoteModelDraft }); }}
                            />
                        }
                    />
                    <SettingsRow
                        icon={<Hash size={14} />}
                        label={t("settings_remote_dimensions")}
                        desc={t("settings_remote_dimensions_desc")}
                        control={
                            <input
                                type="number"
                                className="settings-number-input"
                                value={remoteDimsDraft}
                                placeholder="1024"
                                min={64}
                                max={8192}
                                onChange={(e) => setRemoteDimsDraft(e.target.value)}
                                onBlur={() => updateField({ remote_dimensions: Number.parseInt(remoteDimsDraft, 10) || 1024 })}
                                onKeyDown={(e) => { if (e.key === "Enter") updateField({ remote_dimensions: Number.parseInt(remoteDimsDraft, 10) || 1024 }); }}
                            />
                        }
                    />
                </div>
            )}

            {providerChanged && (
                <div className="settings-provider-warning">
                    ⚠️ {t("settings_provider_changed_warning")}
                </div>
            )}
        </>
    );
}
