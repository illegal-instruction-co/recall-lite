export interface SearchResult {
    path: string;
    snippet: string;
    score: number;
}

export interface IndexingProgress {
    current: number;
    total: number;
    path: string;
}

export interface ContainerItem {
    name: string;
    description: string;
    indexed_paths: string[];
    provider_label: string;
}
