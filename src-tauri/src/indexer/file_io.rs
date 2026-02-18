use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::config::IndexingConfig;

pub fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext,
        "txt"
            | "md"
            | "markdown"
            | "rs"
            | "toml"
            | "json"
            | "jsonc"
            | "json5"
            | "yaml"
            | "yml"
            | "js"
            | "mjs"
            | "cjs"
            | "ts"
            | "mts"
            | "cts"
            | "jsx"
            | "tsx"
            | "py"
            | "pyi"
            | "pyw"
            | "rb"
            | "erb"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "scala"
            | "sc"
            | "groovy"
            | "gradle"
            | "clj"
            | "cljs"
            | "cljc"
            | "c"
            | "cpp"
            | "cc"
            | "cxx"
            | "h"
            | "hpp"
            | "hxx"
            | "hh"
            | "cs"
            | "fs"
            | "fsi"
            | "fsx"
            | "vb"
            | "vbs"
            | "swift"
            | "m"
            | "mm"
            | "dart"
            | "php"
            | "pl"
            | "pm"
            | "lua"
            | "r"
            | "jl"
            | "ex"
            | "exs"
            | "erl"
            | "hrl"
            | "hs"
            | "lhs"
            | "ml"
            | "mli"
            | "elm"
            | "zig"
            | "nim"
            | "v"
            | "d"
            | "sol"
            | "move"
            | "wat"
            | "asm"
            | "s"
            | "pas"
            | "lisp"
            | "el"
            | "rkt"
            | "html"
            | "htm"
            | "xml"
            | "svg"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "styl"
            | "vue"
            | "svelte"
            | "astro"
            | "pug"
            | "ejs"
            | "hbs"
            | "graphql"
            | "gql"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
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
            | "properties"
            | "dockerfile"
            | "makefile"
            | "cmake"
            | "tf"
            | "tfvars"
            | "hcl"
            | "nix"
            | "proto"
            | "lock"
            | "tex"
            | "bib"
            | "rst"
            | "adoc"
    )
}

pub fn is_text_extension_with_config(ext: &str, config: &IndexingConfig) -> bool {
    if config.excluded_extensions.iter().any(|e| e == ext) {
        return false;
    }
    if is_text_extension(ext) {
        return true;
    }
    config.extra_extensions.iter().any(|e| e == ext)
}

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

pub fn read_file_content(path: &Path) -> Option<String> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_FILE_SIZE {
            return None;
        }
    }

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

pub fn read_file_content_with_config(path: &Path, config: &IndexingConfig) -> Option<String> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_FILE_SIZE {
            return None;
        }
    }

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

    if config.excluded_extensions.iter().any(|e| e == &ext) {
        return None;
    }

    if is_text_extension_with_config(&ext, config) || is_dotfile {
        fs::read_to_string(path).ok()
    } else if ext == "pdf" {
        pdf_extract::extract_text(path).ok()
    } else {
        None
    }
}

pub async fn read_file_content_with_ocr(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if super::ocr::is_image_extension(&ext) {
        super::ocr::extract_text_from_image(path).await.ok()
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
