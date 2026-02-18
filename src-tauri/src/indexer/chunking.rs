use regex::Regex;

pub struct ChunkConfig {
    pub max_bytes: usize,
    pub overlap_bytes: usize,
}

pub fn get_chunk_config(ext: &str) -> ChunkConfig {
    match ext {
        "rs" | "py" | "pyi" | "pyw" | "js" | "mjs" | "cjs" | "ts" | "mts" | "cts" | "tsx"
        | "jsx" | "go" | "java" | "kt" | "kts" | "scala" | "sc" | "groovy" | "gradle" | "clj"
        | "cljs" | "cljc" | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" | "hh" | "cs"
        | "fs" | "fsi" | "fsx" | "vb" | "vbs" | "rb" | "erb" | "swift" | "m" | "mm" | "dart"
        | "php" | "pl" | "pm" | "lua" | "r" | "jl" | "ex" | "exs" | "erl" | "hrl" | "hs"
        | "lhs" | "ml" | "mli" | "elm" | "zig" | "nim" | "v" | "d" | "sol" | "move" | "pas"
        | "lisp" | "el" | "rkt" | "asm" | "s" | "wat" => ChunkConfig {
            max_bytes: 1200,
            overlap_bytes: 200,
        },
        "vue" | "svelte" | "astro" => ChunkConfig {
            max_bytes: 1200,
            overlap_bytes: 200,
        },
        "md" | "markdown" | "txt" | "rst" | "adoc" | "tex" => ChunkConfig {
            max_bytes: 800,
            overlap_bytes: 150,
        },
        "toml" | "yaml" | "yml" | "json" | "jsonc" | "json5" | "ini" | "cfg" | "conf" | "env"
        | "properties" | "tf" | "tfvars" | "hcl" | "nix" | "proto" | "graphql" | "gql" => {
            ChunkConfig {
                max_bytes: 600,
                overlap_bytes: 100,
            }
        }
        "csv" | "tsv" | "sql" | "log" | "lock" | "cmake" => ChunkConfig {
            max_bytes: 800,
            overlap_bytes: 150,
        },
        _ => ChunkConfig {
            max_bytes: 800,
            overlap_bytes: 150,
        },
    }
}

fn get_semantic_pattern(ext: &str) -> Option<Regex> {
    let pattern = match ext {
        "rs" => r"\n(?:pub\s+)?(?:async\s+)?(?:fn |struct |enum |impl |trait |mod )",
        "py" | "pyi" | "pyw" => r"\n(?:class |def |async def )",
        "js" | "jsx" | "mjs" | "cjs" => {
            r"\n(?:function |class |export (?:default )?(?:function |class |const |let ))"
        }
        "ts" | "tsx" | "mts" | "cts" => {
            r"\n(?:(?:export )?(?:function |class |interface |type |const |enum |async function ))"
        }
        "go" => r"\n(?:func |type )",
        "java" | "cs" => {
            r"\n\s*(?:public |private |protected )?(?:static )?(?:class |interface |void |int |string |def )"
        }
        "kt" | "kts" => {
            r"\n(?:(?:override |suspend |private |internal |public )?(?:fun |class |object |interface |data class |sealed class |enum class ))"
        }
        "scala" | "sc" => {
            r"\n\s*(?:(?:private |protected )?(?:def |class |object |trait |case class |val |var ))"
        }
        "swift" => {
            r"\n\s*(?:(?:public |private |internal |open )?(?:func |class |struct |enum |protocol |extension ))"
        }
        "dart" => r"\n\s*(?:(?:abstract )?class |void |Future |Stream |[A-Z][a-zA-Z]*\s+[a-z])",
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" | "hh" | "m" | "mm" => {
            r"\n(?:[a-zA-Z_][a-zA-Z0-9_*\s]+\([^)]*\)\s*\{)"
        }
        "rb" | "erb" => r"\n(?:class |module |def )",
        "php" => {
            r"\n\s*(?:(?:public |private |protected |static )?function |class |interface |trait )"
        }
        "lua" => r"\n(?:(?:local )?function )",
        "jl" => r"\n(?:function |macro |struct |module |abstract type )",
        "ex" | "exs" => r"\n\s*(?:def |defp |defmodule |defmacro )",
        "erl" | "hrl" => r"\n[a-z][a-zA-Z0-9_]*\(",
        "hs" | "lhs" => r"\n[a-z][a-zA-Z0-9_']*\s+::",
        "ml" | "mli" => r"\n(?:let |type |module |val )",
        "elm" => r"\n[a-z][a-zA-Z0-9_]*\s+:",
        "fs" | "fsi" | "fsx" => r"\n(?:let |type |module |member )",
        "zig" => r"\n(?:(?:pub )?(?:fn |const |var ))",
        "nim" => r"\n(?:proc |func |method |type |template |macro )",
        "v" => r"\n(?:(?:pub )?(?:fn |struct |enum |interface ))",
        "d" => r"\n(?:[a-zA-Z_][a-zA-Z0-9_*\s]+\([^)]*\)\s*\{)",
        "sol" => r"\n\s*(?:function |contract |interface |library |event |modifier )",
        "clj" | "cljs" | "cljc" | "lisp" | "el" | "rkt" => r"\n\(",
        "pl" | "pm" => r"\n(?:sub |package )",
        "r" => r"\n[a-zA-Z_.][a-zA-Z0-9_.]*\s*<-\s*function",
        "groovy" | "gradle" => r"\n\s*(?:def |class |interface )",
        "vue" | "svelte" | "astro" => r"\n<(?:template|script|style)",
        "pas" => r"\n(?:procedure |function |type |var |begin )",
        "vb" | "vbs" => r"\n\s*(?:Sub |Function |Class |Property |Module )",
        "md" | "markdown" => r"\n#{1,6} ",
        "rst" | "adoc" => r"\n\n",
        "txt" | "tex" | "bib" => r"\n\n",
        "toml" | "ini" | "cfg" => r"\n\[",
        "yaml" | "yml" => r"\n[a-zA-Z_][a-zA-Z0-9_]*:",
        "tf" | "tfvars" | "hcl" => r"\n(?:resource |data |variable |output |module |locals )",
        "nix" => r"\n\s*[a-zA-Z_][a-zA-Z0-9_-]*\s*=",
        "proto" => r"\n(?:message |service |enum |rpc )",
        "graphql" | "gql" => r"\n(?:type |query |mutation |subscription |input |interface |enum )",
        _ => return None,
    };
    Regex::new(pattern).ok()
}

pub fn semantic_chunk_with_overrides(
    text: &str,
    ext: &str,
    chunk_size: Option<usize>,
    chunk_overlap: Option<usize>,
) -> Vec<String> {
    let mut config = get_chunk_config(ext);
    if let Some(size) = chunk_size {
        config.max_bytes = size.max(100);
    }
    if let Some(overlap) = chunk_overlap {
        config.overlap_bytes = overlap;
    }

    let pattern = match get_semantic_pattern(ext) {
        Some(p) => p,
        None => return chunk_with_overlap(text, config.max_bytes, config.overlap_bytes),
    };

    chunk_with_semantic_config(text, &config, &pattern)
}

pub fn semantic_chunk(text: &str, ext: &str) -> Vec<String> {
    let config = get_chunk_config(ext);

    let pattern = match get_semantic_pattern(ext) {
        Some(p) => p,
        None => return chunk_with_overlap(text, config.max_bytes, config.overlap_bytes),
    };

    chunk_with_semantic_config(text, &config, &pattern)
}

fn chunk_with_semantic_config(text: &str, config: &ChunkConfig, pattern: &Regex) -> Vec<String> {
    let mut split_points: Vec<usize> = vec![0];
    for m in pattern.find_iter(text) {
        let pos = m.start();
        if pos > 0 {
            let newline_pos = text[pos..].find('\n').map(|i| pos + i + 1).unwrap_or(pos);
            if newline_pos > *split_points.last().unwrap_or(&0) {
                split_points.push(newline_pos);
            }
        }
    }
    split_points.push(text.len());
    split_points.dedup();

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_last_line = String::new();

    for window in split_points.windows(2) {
        let segment = &text[window[0]..window[1]];

        if !current.is_empty() && current.len() + segment.len() > config.max_bytes {
            if current.len() > config.max_bytes {
                let sub_chunks =
                    chunk_with_overlap(&current, config.max_bytes, config.overlap_bytes);
                if let Some(last) = sub_chunks.last() {
                    prev_last_line = last.lines().last().unwrap_or("").to_string();
                }
                chunks.extend(sub_chunks);
            } else {
                prev_last_line = current.lines().last().unwrap_or("").to_string();
                chunks.push(current.clone());
            }
            current.clear();
            if !prev_last_line.is_empty() {
                current.push_str(&prev_last_line);
                current.push('\n');
            }
        }

        current.push_str(segment);
    }

    if !current.trim().is_empty() {
        if current.len() > config.max_bytes {
            chunks.extend(chunk_with_overlap(
                &current,
                config.max_bytes,
                config.overlap_bytes,
            ));
        } else {
            chunks.push(current);
        }
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

pub fn chunk_with_overlap(text: &str, max_bytes: usize, overlap_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let mut end = (start + max_bytes).min(text.len());

        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }

        if end >= text.len() {
            chunks.push(text[start..].to_string());
            break;
        }

        let slice = &text[start..end];
        let split_at = slice
            .rfind('\n')
            .or_else(|| slice.rfind(". "))
            .or_else(|| slice.rfind(' '))
            .map(|i| start + i + 1)
            .unwrap_or(end);

        chunks.push(text[start..split_at].to_string());

        let rewind = overlap_bytes.min(split_at - start);
        let mut overlap_start = split_at - rewind;
        while overlap_start > start && !text.is_char_boundary(overlap_start) {
            overlap_start += 1;
        }
        if overlap_start <= start {
            overlap_start = split_at;
        }
        start = overlap_start;
    }

    chunks
}

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "shall", "can", "to",
    "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "about", "between",
    "through", "during", "and", "but", "or", "nor", "not", "so", "yet", "it", "its", "this",
    "that", "these", "those", "i", "me", "my", "we", "our", "you", "your", "he", "she", "they",
    "them", "their", "what", "which", "who", "whom", "how", "when", "where", "why", "bir", "ve",
    "ile", "de", "da", "bu", "o", "ne", "nasıl", "nerede", "neden", "için", "gibi", "daha", "en",
    "çok", "var",
];

pub fn expand_query(query: &str) -> Vec<String> {
    let mut variants = Vec::new();
    variants.push(query.to_string());

    let lower = query.to_lowercase();
    if lower != query {
        variants.push(lower.clone());
    }

    let keywords: Vec<&str> = lower
        .split_whitespace()
        .filter(|w| !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();

    if keywords.len() >= 2 && keywords.len() < lower.split_whitespace().count() {
        variants.push(keywords.join(" "));
    }

    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_with_overlap_basic() {
        let text = "Hello world. This is a test. Another sentence here.";
        let chunks = chunk_with_overlap(text, 30, 10);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.len() <= 31));
    }

    #[test]
    fn test_chunk_with_overlap_preserves_content() {
        let text = "ABCDEFGHIJ";
        let chunks = chunk_with_overlap(text, 5, 2);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_chunk_short_text() {
        let text = "Short";
        let chunks = chunk_with_overlap(text, 800, 200);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Short");
    }

    #[test]
    fn test_get_chunk_config_code() {
        let cfg = get_chunk_config("rs");
        assert_eq!(cfg.max_bytes, 1200);
        assert_eq!(cfg.overlap_bytes, 200);
    }

    #[test]
    fn test_get_chunk_config_docs() {
        let cfg = get_chunk_config("md");
        assert_eq!(cfg.max_bytes, 800);
        assert_eq!(cfg.overlap_bytes, 150);
    }

    #[test]
    fn test_get_chunk_config_config() {
        let cfg = get_chunk_config("toml");
        assert_eq!(cfg.max_bytes, 600);
        assert_eq!(cfg.overlap_bytes, 100);
    }

    #[test]
    fn test_get_chunk_config_default() {
        let cfg = get_chunk_config("pdf");
        assert_eq!(cfg.max_bytes, 800);
        assert_eq!(cfg.overlap_bytes, 150);
    }

    #[test]
    fn test_semantic_chunk_rust() {
        let code = "use std::io;\n\nfn main() {\n    println!(\"hello\");\n}\n\npub fn helper() {\n    let x = 1;\n}\n";
        let chunks = semantic_chunk(code, "rs");
        assert!(!chunks.is_empty());
        assert!(chunks.iter().any(|c| c.contains("main")));
        assert!(chunks.iter().any(|c| c.contains("helper")));
    }

    #[test]
    fn test_semantic_chunk_markdown() {
        let md = "# Title\n\nSome intro text.\n\n## Section A\n\nContent A.\n\n## Section B\n\nContent B.\n";
        let chunks = semantic_chunk(md, "md");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_semantic_chunk_fallback() {
        let long_func = format!("fn huge() {{\n{}\n}}", "    let x = 1;\n".repeat(500));
        let chunks = semantic_chunk(&long_func, "rs");
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.len() <= 1500));
    }

    #[test]
    fn test_semantic_chunk_unknown_ext() {
        let text = "Just some plain text content here.";
        let chunks = semantic_chunk(text, "xyz");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_expand_query_basic() {
        let variants = expand_query("Hello World");
        assert!(variants.contains(&"Hello World".to_string()));
        assert!(variants.contains(&"hello world".to_string()));
    }

    #[test]
    fn test_expand_query_stop_words() {
        let variants = expand_query("how to implement search");
        assert!(variants.iter().any(|v| v == "implement search"));
    }

    #[test]
    fn test_expand_query_already_lowercase() {
        let variants = expand_query("hello");
        assert_eq!(variants.len(), 1);
    }

    #[test]
    fn test_expand_query_turkish() {
        let variants = expand_query("bu dosya için arama");
        assert!(variants.iter().any(|v| v == "dosya arama"));
    }
}
