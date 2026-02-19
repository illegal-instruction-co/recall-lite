use std::collections::HashMap;
use std::sync::Arc;

use log::{info, warn};

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
use tokio::sync::Mutex;

use crate::indexer::embedding_provider::RemoteProviderConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum EmbeddingProviderConfig {
    #[serde(rename = "local")]
    Local { model: String },
    #[serde(rename = "remote")]
    Remote(RemoteProviderConfig),
}

impl Default for EmbeddingProviderConfig {
    fn default() -> Self {
        Self::Local {
            model: "MultilingualE5Base".to_string(),
        }
    }
}

impl EmbeddingProviderConfig {
    pub fn provider_label(&self) -> String {
        match self {
            Self::Local { model } => format!("{} (local)", model),
            Self::Remote(rc) => {
                if rc.model.is_empty() {
                    "Remote".to_string()
                } else {
                    rc.model.clone()
                }
            }
        }
    }
}

fn default_provider() -> EmbeddingProviderConfig {
    EmbeddingProviderConfig::default()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IndexingConfig {
    #[serde(default)]
    pub extra_extensions: Vec<String>,
    #[serde(default)]
    pub excluded_extensions: Vec<String>,
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
    #[serde(default = "default_true")]
    pub use_git_history: bool,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            extra_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            chunk_size: None,
            chunk_overlap: None,
            use_git_history: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub description: String,
    pub indexed_paths: Vec<String>,
    #[serde(default)]
    pub embedding_provider: Option<EmbeddingProviderConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(rename = "$schema", default = "default_schema")]
    pub schema: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default = "default_provider")]
    pub embedding_provider: EmbeddingProviderConfig,
    #[serde(default)]
    pub indexing: IndexingConfig,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default)]
    pub launch_at_startup: bool,
    pub containers: HashMap<String, ContainerInfo>,
    pub active_container: String,
    #[serde(default)]
    pub first_run: bool,
    #[serde(default = "default_true")]
    pub use_reranker: bool,
}

fn default_schema() -> String {
    "https://raw.githubusercontent.com/illegal-instruction-co/rememex/main/config.schema.json".to_string()
}

fn default_hotkey() -> String {
    "Alt+Space".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        let mut containers = HashMap::new();
        containers.insert("Default".to_string(), ContainerInfo {
            description: String::new(),
            indexed_paths: Vec::new(),
            embedding_provider: None,
        });
        Self {
            schema: default_schema(),
            embedding_model: "MultilingualE5Base".to_string(),
            embedding_provider: EmbeddingProviderConfig::default(),
            indexing: IndexingConfig::default(),
            hotkey: default_hotkey(),
            always_on_top: true,
            launch_at_startup: false,
            containers,
            active_container: "Default".to_string(),
            first_run: true,
            use_reranker: true,
        }
    }
}

pub fn parse_hotkey(s: &str) -> Shortcut {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut mods = Modifiers::empty();
    let mut key_str = "";

    for part in &parts {
        match part.to_lowercase().as_str() {
            "alt" => mods |= Modifiers::ALT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "win" | "cmd" => mods |= Modifiers::SUPER,
            _ => key_str = part,
        }
    }

    let code = match key_str.to_lowercase().as_str() {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "delete" | "del" => Code::Delete,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "insert" => Code::Insert,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "-" | "minus" => Code::Minus,
        "=" | "equal" => Code::Equal,
        "[" => Code::BracketLeft,
        "]" => Code::BracketRight,
        "\\" | "backslash" => Code::Backslash,
        ";" | "semicolon" => Code::Semicolon,
        "'" | "quote" => Code::Quote,
        "," | "comma" => Code::Comma,
        "." | "period" => Code::Period,
        "/" | "slash" => Code::Slash,
        "`" | "backquote" => Code::Backquote,
        _ => {
            warn!("Unrecognized hotkey key: '{}', falling back to Space", key_str);
            Code::Space
        }
    };

    let mods_opt = if mods.is_empty() { None } else { Some(mods) };
    Shortcut::new(mods_opt, code)
}

pub struct ConfigState {
    pub config: Arc<Mutex<Config>>,
    pub path: std::path::PathBuf,
}

impl ConfigState {
    pub async fn save(&self) -> Result<(), String> {
        let config = self.config.lock().await;
        let content = serde_json::to_string_pretty(&*config).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, content).map_err(|e| e.to_string())?;
        info!("Config saved to {:?}", self.path);
        Ok(())
    }
}

pub fn get_table_name(container: &str) -> String {
    let sanitized: String = container.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
            c.to_string()
        } else {
            format!("{:04x}", c as u32)
        }
    }).collect();
    format!("c_{}", sanitized)
}

pub fn get_embedding_model(name: &str) -> fastembed::EmbeddingModel {
    match name {
        "AllMiniLML6V2" => fastembed::EmbeddingModel::AllMiniLML6V2,
        "MultilingualE5Small" => fastembed::EmbeddingModel::MultilingualE5Small,
        "MultilingualE5Base" => fastembed::EmbeddingModel::MultilingualE5Base,
        _ => fastembed::EmbeddingModel::MultilingualE5Base,
    }
}

pub fn get_local_model_name(config: &Config) -> String {
    match &config.embedding_provider {
        EmbeddingProviderConfig::Local { model } => model.clone(),
        _ => config.embedding_model.clone(),
    }
}

pub fn load_config(config_path: &std::path::Path) -> Config {
    if !config_path.exists() {
        info!("No config found, creating default config");
        let default = Config::default();
        if let Ok(json) = serde_json::to_string_pretty(&default) {
            let _ = std::fs::write(config_path, json);
        }
        return default;
    }
    let content = std::fs::read_to_string(config_path).unwrap_or_default();
    match serde_json::from_str::<Config>(&content) {
        Ok(c) => {
            info!("Config loaded from {:?}", config_path);
            c
        }
        Err(_) => {
            warn!("Config parse failed, attempting migration");
            #[derive(Deserialize)]
            struct OldConfig {
                embedding_model: Option<String>,
                containers: Option<Vec<String>>,
                active_container: Option<String>,
            }
            let migrated = if let Ok(old) = serde_json::from_str::<OldConfig>(&content) {
                let mut containers = HashMap::new();
                if let Some(names) = old.containers {
                    for name in names {
                        containers.insert(name, ContainerInfo {
                            description: String::new(),
                            indexed_paths: Vec::new(),
                            embedding_provider: None,
                        });
                    }
                }
                if containers.is_empty() {
                    containers.insert("Default".to_string(), ContainerInfo {
                        description: String::new(),
                        indexed_paths: Vec::new(),
                        embedding_provider: None,
                    });
                }
                let default_active = containers.keys().next().cloned().unwrap_or_else(|| "Default".to_string());
                let em = old.embedding_model.unwrap_or_else(|| "MultilingualE5Base".to_string());
                Config {
                    schema: default_schema(),
                    embedding_model: em.clone(),
                    embedding_provider: EmbeddingProviderConfig::Local { model: em },
                    indexing: IndexingConfig::default(),
                    hotkey: default_hotkey(),
                    always_on_top: true,
                    launch_at_startup: false,
                    active_container: old.active_container.unwrap_or(default_active),
                    containers,
                    first_run: false,
                    use_reranker: true,
                }
            } else {
                Config::default()
            };
            if let Ok(json) = serde_json::to_string_pretty(&migrated) {
                let _ = std::fs::write(config_path, json);
            }
            migrated
        }
    }
}
