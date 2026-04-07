//! JSON settings file: an additional source for provider/model configuration.
//!
//! Resolution order (first match wins) for the file path:
//!   1. `$CLU_CONFIG` environment variable
//!   2. `./clu.json` in the current working directory
//!   3. `./.clu.json`
//!   4. `$HOME/.clu/settings.json` (or `%USERPROFILE%\.clu\settings.json`)
//!   5. `$XDG_CONFIG_HOME/clu/settings.json` (or `$HOME/.config/clu/settings.json`)
//!   6. `%APPDATA%/clu/settings.json` on Windows
//!
//! Example file:
//! ```json
//! {
//!   "provider": {
//!     "base_url": "http://localhost:11434/v1",
//!     "api_key": "ollama"
//!   },
//!   "model": "llama3.2",
//!   "system_prompt": "You are a terse coding helper."
//! }
//! ```
//!
//! Environment variables (`CLU_BASE_URL`, `CLU_API_KEY`, `CLU_MODEL`) still
//! override anything found in the file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderSettings {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub provider: ProviderSettings,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Settings {
    /// Load settings from the first existing candidate path. Returns
    /// `Settings::default()` (empty) if nothing is found.
    pub fn load() -> Self {
        if Self::find_path().is_none() {
            let _ = Self::write_default();
        }
        Self::load_with_path().map(|(s, _)| s).unwrap_or_default()
    }

    /// Create a default `~/.clu/settings.json` if no config exists yet.
    pub fn write_default() -> std::io::Result<PathBuf> {
        let path = home_dot_clu_path()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let default = Settings {
            provider: ProviderSettings {
                base_url: Some("http://localhost:11434/v1".into()),
                api_key: Some("ollama".into()),
            },
            model: Some("llama3.2".into()),
            system_prompt: None,
        };
        let json = serde_json::to_string_pretty(&default).unwrap();
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Like `load`, but also returns the path the settings came from.
    pub fn load_with_path() -> Option<(Self, PathBuf)> {
        let path = Self::find_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        let parsed: Self = serde_json::from_str(&text).ok()?;
        Some((parsed, path))
    }

    /// Locate the settings file using the documented resolution order.
    pub fn find_path() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("CLU_CONFIG") {
            let p = PathBuf::from(p);
            if p.exists() {
                return Some(p);
            }
        }
        for name in ["clu.json", ".clu.json"] {
            let p = PathBuf::from(name);
            if p.exists() {
                return Some(p);
            }
        }
        if let Some(p) = home_dot_clu_path() {
            if p.exists() {
                return Some(p);
            }
        }
        if let Some(p) = xdg_config_path() {
            if p.exists() {
                return Some(p);
            }
        }
        if let Some(p) = appdata_path() {
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Resolve the effective base URL, with the precedence:
    /// env (`CLU_BASE_URL` / `OPENAI_BASE_URL`) > settings file > default.
    pub fn effective_base_url(&self, default: &str) -> String {
        std::env::var("CLU_BASE_URL")
            .ok()
            .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
            .or_else(|| self.provider.base_url.clone())
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolve the effective API key with the same precedence rules.
    pub fn effective_api_key(&self, default: &str) -> String {
        std::env::var("CLU_API_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .or_else(|| self.provider.api_key.clone())
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolve the effective model id.
    pub fn effective_model(&self, default: &str) -> String {
        std::env::var("CLU_MODEL")
            .ok()
            .or_else(|| self.model.clone())
            .unwrap_or_else(|| default.to_string())
    }
}

fn home_dot_clu_path() -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())?;
    Some(Path::new(&home).join(".clu").join("settings.json"))
}

fn xdg_config_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("clu").join("settings.json"))
}

fn appdata_path() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(Path::new(&base).join("clu").join("settings.json"))
}
