use anyhow::Result;
use sqlx::SqlitePool;

/// Run all database migrations
pub async fn run(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            project_type TEXT NOT NULL,
            last_opened_at TEXT DEFAULT CURRENT_TIMESTAMP,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_migrations_create_tables() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        run(&pool).await.unwrap();

        // Verify settings table exists
        let result: (String,) = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='settings'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(result.0, "settings");

        // Verify projects table exists
        let result: (String,) = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='projects'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(result.0, "projects");
    }
}
