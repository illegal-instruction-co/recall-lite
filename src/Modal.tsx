import { useState, useEffect, useRef, useCallback } from "react";
import { AlertTriangle, Info, X } from "lucide-react";
import "./Modal.css";

interface ModalField {
    key: string;
    label: string;
    placeholder?: string;
    defaultValue?: string;
    type?: "text" | "select" | "password" | "number";
    options?: { value: string; label: string }[];
    visible?: boolean;
}

interface ModalConfig {
    type: "confirm" | "prompt";
    title: string;
    message?: string;
    icon?: "warning" | "info";
    fields?: ModalField[];
    confirmText?: string;
    cancelText?: string;
    confirmVariant?: "danger" | "primary";
}

interface ModalResult {
    confirmed: boolean;
    values?: Record<string, string>;
}

let showModalFn: ((config: ModalConfig) => Promise<ModalResult>) | null = null;

export function useModal() {
    return {
        confirm: (config: Omit<ModalConfig, "type">) =>
            showModalFn?.({ ...config, type: "confirm" }) ?? Promise.resolve({ confirmed: false }),
        prompt: (config: Omit<ModalConfig, "type">) =>
            showModalFn?.({ ...config, type: "prompt" }) ?? Promise.resolve({ confirmed: false }),
    };
}

function buildDefaults(fields?: ModalField[]): Record<string, string> {
    const defaults: Record<string, string> = {};
    if (fields) {
        for (const f of fields) {
            defaults[f.key] = f.defaultValue || "";
        }
    }
    return defaults;
}

function initializeModal(
    cfg: ModalConfig,
    setValues: React.Dispatch<React.SetStateAction<Record<string, string>>>,
    setConfig: React.Dispatch<React.SetStateAction<ModalConfig | null>>,
    resolveRef: React.RefObject<((result: ModalResult) => void) | null>,
    setVisible: React.Dispatch<React.SetStateAction<boolean>>,
    resolve: (result: ModalResult) => void
) {
    setValues(buildDefaults(cfg.fields));
    setConfig(cfg);
    resolveRef.current = resolve;
    requestAnimationFrame(() => setVisible(true));
}

function handleInputKeyDown(
    e: React.KeyboardEvent<HTMLInputElement>,
    values: Record<string, string>,
    fields: ModalField[],
    close: (confirmed: boolean) => void
) {
    if (e.key === "Enter") {
        e.preventDefault();
        const firstVal = values[fields[0].key];
        if (firstVal?.trim()) close(true);
    }
}

export function ModalProvider() {
    const [config, setConfig] = useState<ModalConfig | null>(null);
    const [values, setValues] = useState<Record<string, string>>({});
    const [visible, setVisible] = useState(false);
    const resolveRef = useRef<((result: ModalResult) => void) | null>(null);
    const firstInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        showModalFn = (cfg) => new Promise<ModalResult>((resolve) => {
            initializeModal(cfg, setValues, setConfig, resolveRef, setVisible, resolve);
        });
        return () => { showModalFn = null; };
    }, []);

    useEffect(() => {
        if (visible && firstInputRef.current) {
            setTimeout(() => firstInputRef.current?.focus(), 80);
        }
    }, [visible]);

    const close = useCallback((confirmed: boolean) => {
        setVisible(false);
        setTimeout(() => {
            resolveRef.current?.({ confirmed, values: confirmed ? values : undefined });
            setConfig(null);
            resolveRef.current = null;
        }, 180);
    }, [values]);

    useEffect(() => {
        if (!config) return;
        const handler = (e: KeyboardEvent) => {
            if (e.key === "Escape") close(false);
            if (e.key === "Enter" && config.type === "confirm") close(true);
        };
        globalThis.addEventListener("keydown", handler);
        return () => globalThis.removeEventListener("keydown", handler);
    }, [config, close]);

    if (!config) return null;

    const IconComponent = config.icon === "warning" ? AlertTriangle : Info;
    const iconClass = config.icon === "warning" ? "modal-icon warning" : "modal-icon info";

    const handleBackdropClick = (e: React.MouseEvent<HTMLDivElement>) => {
        if (e.target === e.currentTarget) close(false);
    };

    return (
        <div className={`modal-overlay ${visible ? "visible" : ""}`} role="none" onClick={handleBackdropClick} onKeyDown={(e) => { if (e.key === "Escape") close(false); }}>
            <dialog className={`modal-container ${visible ? "visible" : ""}`} open={visible}>
                <div className="modal-header">
                    <div className={iconClass}>
                        <IconComponent size={18} />
                    </div>
                    <h3 className="modal-title">{config.title}</h3>
                    <button type="button" className="modal-close" onClick={() => close(false)}>
                        <X size={14} />
                    </button>
                </div>

                {config.message && (
                    <p className="modal-message">{config.message}</p>
                )}

                {config.type === "prompt" && config.fields && (
                    <div className="modal-fields">
                        {config.fields.map((field, i) => {
                            if (field.visible === false) return null;
                            const fieldType = field.type || "text";

                            if (fieldType === "select" && field.options) {
                                return (
                                    <div key={field.key} className="modal-field">
                                        <label className="modal-label">{field.label}</label>
                                        <select
                                            className="modal-input"
                                            value={values[field.key] || ""}
                                            onChange={(e) => setValues((v) => ({ ...v, [field.key]: e.target.value }))}
                                        >
                                            {field.options.map((opt) => (
                                                <option key={opt.value} value={opt.value}>{opt.label}</option>
                                            ))}
                                        </select>
                                    </div>
                                );
                            }

                            let inputType: "text" | "number" | "password";

                            if (fieldType === "password") {
                                inputType = "password";
                            } else if (fieldType === "number") {
                                inputType = "number";
                            } else {
                                inputType = "text";
                            }

                            return (
                                <div key={field.key} className="modal-field">
                                    <label className="modal-label">{field.label}</label>
                                    <input
                                        ref={i === 0 ? firstInputRef : undefined}
                                        className="modal-input"
                                        type={inputType}
                                        placeholder={field.placeholder}
                                        value={values[field.key] || ""}
                                        onChange={(e) =>
                                            setValues((v) => ({ ...v, [field.key]: e.target.value }))
                                        }
                                        onKeyDown={(e) =>
                                            handleInputKeyDown(e, values, config.fields!, close)
                                        }
                                    />
                                </div>
                            );
                        })}
                    </div>
                )}

                <div className="modal-actions">
                    <button type="button" className="modal-btn secondary" onClick={() => close(false)}>
                        {config.cancelText || "Cancel"}
                    </button>
                    <button
                        type="button"
                        className={`modal-btn ${config.confirmVariant === "danger" ? "danger" : "primary"}`}
                        onClick={() => close(true)}
                    >
                        {config.confirmText || "OK"}
                    </button>
                </div>
            </dialog>
        </div>
    );
}
