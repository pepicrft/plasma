use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Project type derived from the project file path
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Xcode,
    Android,
}

impl ProjectType {
    /// Infer project type from a file path
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy();

        if name.ends_with(".xcworkspace") || name.ends_with(".xcodeproj") {
            Some(ProjectType::Xcode)
        } else if name == "build.gradle" || name == "build.gradle.kts" {
            Some(ProjectType::Android)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "projects")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Path to the project file (.xcworkspace, .xcodeproj, or build.gradle)
    #[sea_orm(unique)]
    pub path: String,
    pub name: String,
    pub last_opened_at: Option<String>,
    pub created_at: Option<String>,
}

impl Model {
    /// Get the project type inferred from the path
    pub fn project_type(&self) -> Option<ProjectType> {
        ProjectType::from_path(Path::new(&self.path))
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
