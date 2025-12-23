mod migrations;
mod settings;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

pub use settings::SettingsRepository;

/// Database connection pool
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(path: &Path) -> Result<Self> {
        let path_str = path.to_string_lossy();

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path_str))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        migrations::run(&db.pool).await?;

        Ok(db)
    }

    /// Get the underlying pool for direct queries
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get settings repository
    pub fn settings(&self) -> SettingsRepository {
        SettingsRepository::new(self.pool.clone())
    }
}
