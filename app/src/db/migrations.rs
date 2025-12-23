use anyhow::Result;
use sqlx::SqlitePool;

/// Run all database migrations using sqlx's built-in migrator.
/// Migrations are embedded at compile time from the `migrations/` directory.
/// Progress is tracked in the `_sqlx_migrations` table.
pub async fn run(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
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

        // Verify migrations are tracked
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn test_migrations_are_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        // Run migrations twice - should not fail
        run(&pool).await.unwrap();
        run(&pool).await.unwrap();

        // Still only 2 migrations recorded
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);
    }
}
