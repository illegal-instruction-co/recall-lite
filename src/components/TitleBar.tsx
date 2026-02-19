import { Minus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./TitleBar.css";

export default function TitleBar() {
    const appWindow = getCurrentWindow();

    return (
        <div className="titlebar" data-tauri-drag-region>
            <div className="titlebar-buttons">
                <button
                    type="button"
                    className="titlebar-btn titlebar-minimize"
                    onClick={() => appWindow.minimize()}
                    aria-label="Minimize"
                >
                    <Minus size={10} />
                </button>
                <button
                    type="button"
                    className="titlebar-btn titlebar-close"
                    onClick={() => appWindow.hide()}
                    aria-label="Close"
                >
                    <X size={10} />
                </button>
            </div>
        </div>
    );
}
