import { ReactNode } from "react";
import "./SettingsRow.css";

interface SettingsRowProps {
    icon: ReactNode;
    label: string;
    desc: string;
    control: ReactNode;
    hotkey?: boolean;
    note?: string;
}

export function SettingsRow({ icon, label, desc, control, hotkey, note }: Readonly<SettingsRowProps>) {
    return (
        <>
            <div className={`settings-row${hotkey ? " hotkey-row" : ""}`}>
                <div className="settings-row-info">
                    <span className="settings-row-icon">{icon}</span>
                    <div>
                        <span className="settings-row-label">{label}</span>
                        <span className="settings-row-desc">{desc}</span>
                    </div>
                </div>
                {control}
            </div>
            {note && <span className="settings-row-note">{note}</span>}
        </>
    );
}

interface SettingsToggleProps {
    label: string;
    checked: boolean;
    onChange: (checked: boolean) => void;
}

export function SettingsToggle({ label, checked, onChange }: Readonly<SettingsToggleProps>) {
    return (
        <label className="toggle" aria-label={label}>
            <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
            <span className="toggle-slider" />
        </label>
    );
}
