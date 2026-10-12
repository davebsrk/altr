use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    #[default]
    Valorant,
    League,
}

/// Number of accent colors the UI offers; `accent` is an index into that list.
pub const ACCENT_COUNT: usize = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeMode,
    pub accent: usize,
    pub default_game: Game,
    /// Ask before switching when an account is clicked in the sidebar.
    pub confirm_switch: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { theme: ThemeMode::default(), accent: 0, default_game: Game::default(), confirm_switch: true }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::load_from(&crate::data_dir().join("config.toml"))
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&crate::data_dir().join("config.toml"))
    }

    /// Missing or unreadable files fall back to defaults instead of failing startup.
    pub fn load_from(path: &Path) -> Self {
        let mut config: Self = fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        if config.accent >= ACCENT_COUNT {
            config.accent = 0;
        }
        config
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(io::Error::other)?;
        crate::write_atomic(path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_fallbacks() {
        let dir = std::env::temp_dir().join(format!("valo-cfg-{}", std::process::id()));
        let path = dir.join("config.toml");

        assert_eq!(Config::load_from(&path), Config::default());

        let config = Config { theme: ThemeMode::Dark, accent: 3, default_game: Game::League, confirm_switch: false };
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path), config);

        // Files written before a field existed keep that field's default.
        fs::write(&path, "accent = 2").unwrap();
        assert_eq!(Config::load_from(&path), Config { accent: 2, ..Config::default() });

        fs::write(&path, "accent = 99\ntheme = \"nope\"").unwrap();
        assert_eq!(Config::load_from(&path), Config::default());

        fs::remove_dir_all(dir).unwrap();
    }
}
