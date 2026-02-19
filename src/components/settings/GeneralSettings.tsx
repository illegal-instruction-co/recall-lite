import { Pin, Rocket, Keyboard, Globe, Layers } from "lucide-react";
import { useLocale } from "../../i18n";
import { SettingsRow, SettingsToggle } from "./SettingsRow";
import "./GeneralSettings.css";

interface AppConfig {
    always_on_top: boolean;
    launch_at_startup: boolean;
    hotkey: string;
    use_reranker: boolean;
}

const localeLabels: Record<string, string> = { en: "English", tr: "Türkçe" };

interface Props {
    config: AppConfig;
    hotkeyDraft: string;
    hotkeyDirty: boolean;
    onHotkeyChange: (v: string) => void;
    updateField: (updates: Partial<AppConfig>) => Promise<void>;
    setHotkeyDirty: (v: boolean) => void;
}

export default function GeneralSettings({ config, hotkeyDraft, hotkeyDirty, onHotkeyChange, updateField, setHotkeyDirty }: Readonly<Props>) {
    const { t, locale, setLocale, availableLocales } = useLocale();

    return (
        <div className="settings-group">
            <SettingsRow
                icon={<Pin size={14} />}
                label={t("settings_always_on_top")}
                desc={t("settings_always_on_top_desc")}
                control={
                    <SettingsToggle
                        label={t("settings_always_on_top")}
                        checked={config.always_on_top}
                        onChange={(v) => updateField({ always_on_top: v })}
                    />
                }
            />

            <SettingsRow
                icon={<Rocket size={14} />}
                label={t("settings_launch_startup")}
                desc={t("settings_launch_startup_desc")}
                control={
                    <SettingsToggle
                        label={t("settings_launch_startup")}
                        checked={config.launch_at_startup}
                        onChange={(v) => updateField({ launch_at_startup: v })}
                    />
                }
            />

            <SettingsRow
                icon={<Keyboard size={14} />}
                label={t("settings_hotkey")}
                desc={t("settings_hotkey_desc")}
                hotkey
                note={hotkeyDirty ? t("settings_restart_required") : undefined}
                control={
                    <div className="hotkey-input-wrapper">
                        <input
                            type="text"
                            className="hotkey-input"
                            value={hotkeyDraft}
                            onChange={(e) => {
                                onHotkeyChange(e.target.value);
                                setHotkeyDirty(e.target.value !== config.hotkey);
                            }}
                            onKeyDown={(e) => {
                                if (e.key === "Enter" && hotkeyDirty) {
                                    e.preventDefault();
                                    updateField({ hotkey: hotkeyDraft }).then(() => setHotkeyDirty(false));
                                }
                            }}
                            spellCheck={false}
                        />
                        {hotkeyDirty && (
                            <button
                                className="hotkey-save"
                                onClick={() => updateField({ hotkey: hotkeyDraft }).then(() => setHotkeyDirty(false))}
                            >
                                ↵
                            </button>
                        )}
                    </div>
                }
            />

            <SettingsRow
                icon={<Globe size={14} />}
                label={t("settings_language")}
                desc={t("settings_language_desc")}
                control={
                    <select
                        className="settings-select"
                        value={locale}
                        aria-label={t("settings_language")}
                        onChange={(e) => setLocale(e.target.value)}
                    >
                        {availableLocales.map((loc) => (
                            <option key={loc} value={loc}>
                                {localeLabels[loc] ?? loc}
                            </option>
                        ))}
                    </select>
                }
            />

            <SettingsRow
                icon={<Layers size={14} />}
                label={t("settings_use_reranker")}
                desc={t("settings_use_reranker_desc")}
                control={
                    <SettingsToggle
                        label={t("settings_use_reranker")}
                        checked={config.use_reranker}
                        onChange={(v) => updateField({ use_reranker: v })}
                    />
                }
            />
        </div>
    );
}
