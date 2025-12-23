use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub project_type: String,
    pub last_opened_at: String,
}

/// Repository for project operations
#[derive(Clone)]
pub struct ProjectsRepository {
    pool: SqlitePool,
}

impl ProjectsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Save or update a project (updates last_opened_at if exists)
    pub async fn upsert(&self, path: &str, name: &str, project_type: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO projects (path, name, project_type, last_opened_at)
            VALUES (?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(path) DO UPDATE SET
                name = excluded.name,
                project_type = excluded.project_type,
                last_opened_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(path)
        .bind(name)
        .bind(project_type)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get recent projects ordered by last opened
    pub async fn get_recent(&self, limit: i64) -> Result<Vec<Project>> {
        let projects = sqlx::query_as::<_, (i64, String, String, String, String)>(
            r#"
            SELECT id, path, name, project_type, last_opened_at
            FROM projects
            ORDER BY last_opened_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(projects
            .into_iter()
            .map(|(id, path, name, project_type, last_opened_at)| Project {
                id,
                path,
                name,
                project_type,
                last_opened_at,
            })
            .collect())
    }

    /// Search projects by path prefix
    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<Project>> {
        let pattern = format!("%{}%", query);
        let projects = sqlx::query_as::<_, (i64, String, String, String, String)>(
            r#"
            SELECT id, path, name, project_type, last_opened_at
            FROM projects
            WHERE path LIKE ?
            ORDER BY last_opened_at DESC
            LIMIT ?
            "#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(projects
            .into_iter()
            .map(|(id, path, name, project_type, last_opened_at)| Project {
                id,
                path,
                name,
                project_type,
                last_opened_at,
            })
            .collect())
    }

    /// Delete a project by path
    pub async fn delete(&self, path: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM projects WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_projects_crud() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).await.unwrap();
        let projects = db.projects();

        // Insert a project
        projects
            .upsert("/path/to/project", "MyApp", "xcode")
            .await
            .unwrap();

        // Get recent projects
        let recent = projects.get_recent(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].path, "/path/to/project");
        assert_eq!(recent[0].name, "MyApp");
        assert_eq!(recent[0].project_type, "xcode");

        // Update the same project
        projects
            .upsert("/path/to/project", "MyApp Updated", "xcode")
            .await
            .unwrap();

        let recent = projects.get_recent(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "MyApp Updated");

        // Search
        let results = projects.search("/path", 10).await.unwrap();
        assert_eq!(results.len(), 1);

        // Delete
        let deleted = projects.delete("/path/to/project").await.unwrap();
        assert!(deleted);

        let recent = projects.get_recent(10).await.unwrap();
        assert!(recent.is_empty());
    }
}
