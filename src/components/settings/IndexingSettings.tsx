import { GitBranch, Ruler, FilePlus, FileX } from "lucide-react";
import { useLocale } from "../../i18n";
import { SettingsRow, SettingsToggle } from "./SettingsRow";
import "./IndexingSettings.css";

interface IndexingConfig {
    use_git_history: boolean;
    chunk_size: number | null;
    chunk_overlap: number | null;
}

interface Props {
    config: IndexingConfig;
    extraExtDraft: string;
    excludedExtDraft: string;
    setExtraExtDraft: (v: string) => void;
    setExcludedExtDraft: (v: string) => void;
    updateField: (updates: Record<string, unknown>) => Promise<void>;
}

function parseExtensions(raw: string): string[] {
    return raw
        .split(/[,\s]+/)
        .map((s) => s.trim().replace(/^\./, ""))
        .filter((s) => s.length > 0);
}

export default function IndexingSettings({
    config, extraExtDraft, excludedExtDraft,
    setExtraExtDraft, setExcludedExtDraft, updateField,
}: Readonly<Props>) {
    const { t } = useLocale();

    return (
        <>
            <SettingsRow
                icon={<GitBranch size={14} />}
                label={t("settings_git_history")}
                desc={t("settings_git_history_desc")}
                control={
                    <SettingsToggle
                        label={t("settings_git_history")}
                        checked={config.use_git_history}
                        onChange={(v) => updateField({ use_git_history: v })}
                    />
                }
            />

            <SettingsRow
                icon={<Ruler size={14} />}
                label={t("settings_chunk_size")}
                desc={t("settings_chunk_desc")}
                control={
                    <div className="settings-number-group">
                        <input
                            type="number"
                            className="settings-number-input"
                            value={config.chunk_size ?? ""}
                            placeholder="512"
                            aria-label={t("settings_chunk_size")}
                            min={64}
                            max={4096}
                            onChange={(e) => {
                                const v = e.target.value ? Number.parseInt(e.target.value, 10) : null;
                                updateField({ chunk_size: v });
                            }}
                        />
                        <span className="settings-number-label">{t("settings_chunk_overlap")}</span>
                        <input
                            type="number"
                            className="settings-number-input"
                            value={config.chunk_overlap ?? ""}
                            placeholder="64"
                            aria-label={t("settings_chunk_overlap")}
                            min={0}
                            max={512}
                            onChange={(e) => {
                                const v = e.target.value ? Number.parseInt(e.target.value, 10) : null;
                                updateField({ chunk_overlap: v });
                            }}
                        />
                    </div>
                }
            />

            <SettingsRow
                icon={<FilePlus size={14} />}
                label={t("settings_extra_ext")}
                desc={t("settings_extra_ext_desc")}
                control={
                    <input
                        type="text"
                        className="settings-ext-input"
                        value={extraExtDraft}
                        placeholder=".xyz, .abc"
                        aria-label={t("settings_extra_ext")}
                        spellCheck={false}
                        onChange={(e) => setExtraExtDraft(e.target.value)}
                        onBlur={() => updateField({ extra_extensions: parseExtensions(extraExtDraft) })}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") updateField({ extra_extensions: parseExtensions(extraExtDraft) });
                        }}
                    />
                }
            />

            <SettingsRow
                icon={<FileX size={14} />}
                label={t("settings_excluded_ext")}
                desc={t("settings_excluded_ext_desc")}
                control={
                    <input
                        type="text"
                        className="settings-ext-input"
                        value={excludedExtDraft}
                        placeholder=".log, .tmp"
                        aria-label={t("settings_excluded_ext")}
                        spellCheck={false}
                        onChange={(e) => setExcludedExtDraft(e.target.value)}
                        onBlur={() => updateField({ excluded_extensions: parseExtensions(excludedExtDraft) })}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") updateField({ excluded_extensions: parseExtensions(excludedExtDraft) });
                        }}
                    />
                }
            />
        </>
    );
}
