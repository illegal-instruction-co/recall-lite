use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext,
        "txt"
            | "md"
            | "markdown"
            | "rs"
            | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "py"
            | "rb"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "html"
            | "htm"
            | "xml"
            | "svg"
            | "css"
            | "scss"
            | "less"
            | "sql"
            | "sh"
            | "bash"
            | "ps1"
            | "bat"
            | "cmd"
            | "csv"
            | "tsv"
            | "log"
            | "ini"
            | "cfg"
            | "conf"
            | "env"
            | "dockerfile"
            | "makefile"
            | "tex"
            | "bib"
            | "rst"
            | "adoc"
    )
}

pub fn read_file_content(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let is_dotfile = matches!(
        file_name.as_str(),
        "dockerfile" | "makefile" | ".gitignore" | ".env" | ".editorconfig"
    );

    if is_text_extension(&ext) || is_dotfile {
        fs::read_to_string(path).ok()
    } else if ext == "pdf" {
        pdf_extract::extract_text(path).ok()
    } else {
        None
    }
}

pub fn read_file_content_with_ocr(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if super::ocr::is_image_extension(&ext) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(super::ocr::extract_text_from_image(path))
                .ok()
        })
    } else {
        read_file_content(path)
    }
}

pub fn get_file_mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_text_extension() {
        assert!(is_text_extension("py"));
        assert!(is_text_extension("tsx"));
        assert!(is_text_extension("rs"));
        assert!(is_text_extension("sql"));
        assert!(!is_text_extension("exe"));
        assert!(!is_text_extension("png"));
    }
}
