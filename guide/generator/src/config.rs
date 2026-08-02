// guide.toml: what the Guide is made of, and where it links back to.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub sidebar: Sidebar,
    pub parts: Vec<Part>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sidebar {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Deserialize)]
pub struct Part {
    pub title: String,
    pub source: PathBuf,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))
    }

    pub fn sidebar_class(&self) -> &'static str {
        match self.sidebar {
            Sidebar::Left => "sidebar-left",
            Sidebar::Right => "sidebar-right",
        }
    }
}
