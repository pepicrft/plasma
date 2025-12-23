use anyhow::Result;
use sqlx::SqlitePool;

/// Repository for settings operations
#[derive(Clone)]
pub struct SettingsRepository {
    pool: SqlitePool,
}

impl SettingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a setting by key
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(|(v,)| v))
    }

    /// Set a setting value
    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a setting by key
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsRepository;
    use crate::db::Database;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_settings_crud() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).await.unwrap();
        let settings = db.settings();

        // Set a value
        settings.set("test_key", "test_value").await.unwrap();

        // Get the value
        let value = settings.get("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Update the value
        settings.set("test_key", "new_value").await.unwrap();
        let value = settings.get("test_key").await.unwrap();
        assert_eq!(value, Some("new_value".to_string()));

        // Delete the value
        let deleted = settings.delete("test_key").await.unwrap();
        assert!(deleted);

        let value = settings.get("test_key").await.unwrap();
        assert_eq!(value, None);
    }
}
