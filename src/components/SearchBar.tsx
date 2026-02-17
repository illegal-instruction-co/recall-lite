import { Search, FolderPlus, Loader2 } from "lucide-react";
import { useLocale } from "../i18n";

interface SearchBarProps {
    query: string;
    onQueryChange: (value: string) => void;
    activeContainer: string;
    isIndexing: boolean;
    onPickFolder: () => void;
    inputRef: React.RefObject<HTMLInputElement | null>;
}

export default function SearchBar({
    query, onQueryChange, activeContainer, isIndexing, onPickFolder, inputRef,
}: SearchBarProps) {
    const { t } = useLocale();

    return (
        <div className="search-wrapper shrink-0">
            <div className="relative">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 text-[--color-text-tertiary] pointer-events-none" size={18} />
                <input
                    ref={inputRef}
                    type="text"
                    value={query}
                    onChange={(e) => onQueryChange(e.target.value)}
                    placeholder={t("search_placeholder", { container: activeContainer })}
                    className="search-input"
                    autoFocus
                />
                <button
                    onClick={onPickFolder}
                    className="absolute right-4 top-1/2 -translate-y-1/2 p-2 rounded-md hover:bg-[--color-control-fill-secondary] text-[--color-text-secondary] transition-colors"
                    title={t("index_folder_title", { container: activeContainer })}
                >
                    {isIndexing ? <Loader2 className="animate-spin" size={18} /> : <FolderPlus size={18} />}
                </button>
            </div>
        </div>
    );
}
