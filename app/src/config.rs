use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub debug: bool,
    pub database_path: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 4000,
            debug: false,
            database_path: None,
            env: HashMap::new(),
        }
    }
}

impl Config {
    /// Load configuration from the default config file location
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("dev", "plasma", "Plasma")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir)?;

        Ok(config_dir.join("app.toml"))
    }

    /// Get the database path, using a default if not specified
    pub fn get_database_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.database_path {
            Ok(PathBuf::from(path))
        } else {
            let proj_dirs = ProjectDirs::from("dev", "plasma", "Plasma")
                .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;

            let data_dir = proj_dirs.data_dir();
            fs::create_dir_all(data_dir)?;

            Ok(data_dir.join("plasma.db"))
        }
    }

    /// Save the configuration to the default config file location
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.port, 4000);
        assert!(!config.debug);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
            port = 8080
            debug = true
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.port, 8080);
        assert!(config.debug);
    }
}
