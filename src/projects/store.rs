//! Project CRUD storage (SQLite + Postgres).
//!
//! Per-method dispatch on `Arc<DbPool>` per Phase 11.2. All methods are
//! Pattern B (placeholder divergence: `?` for SQLite, `$N` for Postgres);
//! the schema differs in `created_at`/`updated_at` types (DATETIME for
//! SQLite, TIMESTAMPTZ for Postgres) so per-backend row readers handle the
//! deserialization. `CURRENT_TIMESTAMP` works on both backends as-is.

use crate::db::DbPool;
use crate::error::Result;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row as _;
use std::path::Path;
use std::sync::Arc;

// Enums

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::Archived => "archived",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(ProjectStatus::Active),
            "archived" => Some(ProjectStatus::Archived),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Per-project settings overrides. Each field is optional — `None` means
/// "inherit from the agent-level `ProjectsConfig`". Stored as JSON in the
/// `settings` column of the `projects` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_worktrees: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_name_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_create_worktrees: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_discover_repos: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_discover_worktrees: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_usage_warning_threshold: Option<u64>,
}

// Domain types

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub tags: Vec<String>,
    pub root_path: String,
    pub logo_path: Option<String>,
    pub settings: Value,
    pub status: ProjectStatus,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Project {
    /// Deserialize the `settings` JSON blob into a typed `ProjectSettings`.
    /// Returns defaults if the blob is empty or malformed.
    pub fn typed_settings(&self) -> ProjectSettings {
        serde_json::from_value(self.settings.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProjectRepo {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub remote_url: String,
    pub default_branch: String,
    /// Currently checked-out branch (may differ from `default_branch`).
    pub current_branch: Option<String>,
    pub description: String,
    pub disk_usage_bytes: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProjectWorktree {
    pub id: String,
    pub project_id: String,
    pub repo_id: String,
    pub name: String,
    pub path: String,
    pub branch: String,
    pub created_by: String,
    pub disk_usage_bytes: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Full project with nested repos and worktrees for API responses.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProjectWithRelations {
    #[serde(flatten)]
    pub project: Project,
    pub repos: Vec<ProjectRepo>,
    pub worktrees: Vec<ProjectWorktreeWithRepo>,
}

/// Worktree with the source repo name resolved.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProjectWorktreeWithRepo {
    #[serde(flatten)]
    pub worktree: ProjectWorktree,
    pub repo_name: String,
}

// Input types

#[derive(Debug, Clone)]
pub struct CreateProjectInput {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub tags: Vec<String>,
    pub root_path: String,
    pub settings: Value,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateProjectInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub tags: Option<Vec<String>>,
    pub logo_path: Option<String>,
    pub settings: Option<Value>,
    pub status: Option<ProjectStatus>,
}

#[derive(Debug, Clone)]
pub struct CreateRepoInput {
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub remote_url: String,
    pub default_branch: String,
    pub current_branch: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorktreeInput {
    pub project_id: String,
    pub repo_id: String,
    pub name: String,
    pub path: String,
    pub branch: String,
    pub created_by: String,
}

// Store

#[derive(Debug, Clone)]
pub struct ProjectStore {
    pool: Arc<DbPool>,
}

impl ProjectStore {
    pub fn new(pool: Arc<DbPool>) -> Self {
        Self { pool }
    }

    // -- Projects -----------------------------------------------------------

    pub async fn create_project(&self, input: CreateProjectInput) -> Result<Project> {
        let id = uuid::Uuid::new_v4().to_string();
        let tags_json = serde_json::to_string(&input.tags).context("failed to serialize tags")?;
        let settings_json =
            serde_json::to_string(&input.settings).context("failed to serialize settings")?;

        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                r#"
                INSERT INTO projects (id, name, description, icon, tags, root_path, settings, status)
                VALUES (?, ?, ?, ?, ?, ?, ?, 'active')
                "#,
            )
            .bind(&id)
            .bind(&input.name)
            .bind(&input.description)
            .bind(&input.icon)
            .bind(&tags_json)
            .bind(&input.root_path)
            .bind(&settings_json)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to insert project")?,
            DbPool::Postgres(p) => sqlx::query(
                r#"
                INSERT INTO projects (id, name, description, icon, tags, root_path, settings, status)
                VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')
                "#,
            )
            .bind(&id)
            .bind(&input.name)
            .bind(&input.description)
            .bind(&input.icon)
            .bind(&tags_json)
            .bind(&input.root_path)
            .bind(&settings_json)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to insert project")?,
        };

        Ok(self
            .get_project(&id)
            .await?
            .context("project not found after insert")?)
    }

    pub async fn get_project(&self, project_id: &str) -> Result<Option<Project>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query("SELECT * FROM projects WHERE id = ?")
                .bind(project_id)
                .fetch_optional(p)
                .await
                .context("failed to fetch project")?
                .map(|r| row_to_project_sqlite(&r))
                .transpose(),
            DbPool::Postgres(p) => sqlx::query("SELECT * FROM projects WHERE id = $1")
                .bind(project_id)
                .fetch_optional(p)
                .await
                .context("failed to fetch project")?
                .map(|r| row_to_project_pg(&r))
                .transpose(),
        }
    }

    pub async fn list_projects(&self, status: Option<ProjectStatus>) -> Result<Vec<Project>> {
        match &*self.pool {
            DbPool::Sqlite(p) => {
                let rows = if let Some(status) = status {
                    sqlx::query(
                        "SELECT * FROM projects WHERE status = ? ORDER BY sort_order ASC, updated_at DESC",
                    )
                    .bind(status.as_str())
                    .fetch_all(p)
                    .await
                    .context("failed to list projects")?
                } else {
                    sqlx::query("SELECT * FROM projects ORDER BY sort_order ASC, updated_at DESC")
                        .fetch_all(p)
                        .await
                        .context("failed to list projects")?
                };
                rows.iter().map(row_to_project_sqlite).collect()
            }
            DbPool::Postgres(p) => {
                let rows = if let Some(status) = status {
                    sqlx::query(
                        "SELECT * FROM projects WHERE status = $1 ORDER BY sort_order ASC, updated_at DESC",
                    )
                    .bind(status.as_str())
                    .fetch_all(p)
                    .await
                    .context("failed to list projects")?
                } else {
                    sqlx::query("SELECT * FROM projects ORDER BY sort_order ASC, updated_at DESC")
                        .fetch_all(p)
                        .await
                        .context("failed to list projects")?
                };
                rows.iter().map(row_to_project_pg).collect()
            }
        }
    }

    /// Update the sort_order for a list of projects in a single transaction.
    /// The caller passes IDs in the desired order; each gets sequential order values.
    pub async fn reorder_projects(&self, ids: &[String]) -> Result<()> {
        match &*self.pool {
            DbPool::Sqlite(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .context("failed to begin reorder transaction")?;
                for (order, id) in ids.iter().enumerate() {
                    sqlx::query("UPDATE projects SET sort_order = ? WHERE id = ?")
                        .bind(order as i64)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .context("failed to update project sort_order")?;
                }
                tx.commit()
                    .await
                    .context("failed to commit reorder transaction")?;
            }
            DbPool::Postgres(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .context("failed to begin reorder transaction")?;
                for (order, id) in ids.iter().enumerate() {
                    sqlx::query("UPDATE projects SET sort_order = $1 WHERE id = $2")
                        .bind(order as i64)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .context("failed to update project sort_order")?;
                }
                tx.commit()
                    .await
                    .context("failed to commit reorder transaction")?;
            }
        }
        Ok(())
    }

    pub async fn update_project(
        &self,
        project_id: &str,
        input: UpdateProjectInput,
    ) -> Result<Option<Project>> {
        let existing = self.get_project(project_id).await?;
        let Some(existing) = existing else {
            return Ok(None);
        };

        let name = input.name.unwrap_or(existing.name);
        let description = input.description.unwrap_or(existing.description);
        let icon = input.icon.unwrap_or(existing.icon);
        let tags = input.tags.unwrap_or(existing.tags);
        let tags_json = serde_json::to_string(&tags).context("failed to serialize tags")?;
        let logo_path = input.logo_path.or(existing.logo_path);
        let settings = input.settings.unwrap_or(existing.settings);
        let settings_json =
            serde_json::to_string(&settings).context("failed to serialize settings")?;
        let status = input.status.unwrap_or(existing.status);

        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                r#"
                UPDATE projects
                SET name = ?, description = ?, icon = ?, tags = ?, logo_path = ?,
                    settings = ?, status = ?, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                "#,
            )
            .bind(&name)
            .bind(&description)
            .bind(&icon)
            .bind(&tags_json)
            .bind(&logo_path)
            .bind(&settings_json)
            .bind(status.as_str())
            .bind(project_id)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to update project")?,
            DbPool::Postgres(p) => sqlx::query(
                r#"
                UPDATE projects
                SET name = $1, description = $2, icon = $3, tags = $4, logo_path = $5,
                    settings = $6, status = $7, updated_at = CURRENT_TIMESTAMP
                WHERE id = $8
                "#,
            )
            .bind(&name)
            .bind(&description)
            .bind(&icon)
            .bind(&tags_json)
            .bind(&logo_path)
            .bind(&settings_json)
            .bind(status.as_str())
            .bind(project_id)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to update project")?,
        };

        self.get_project(project_id).await
    }

    pub async fn delete_project(&self, project_id: &str) -> Result<bool> {
        let rows_affected = match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query("DELETE FROM projects WHERE id = ?")
                .bind(project_id)
                .execute(p)
                .await
                .context("failed to delete project")?
                .rows_affected(),
            DbPool::Postgres(p) => sqlx::query("DELETE FROM projects WHERE id = $1")
                .bind(project_id)
                .execute(p)
                .await
                .context("failed to delete project")?
                .rows_affected(),
        };
        Ok(rows_affected > 0)
    }

    /// Load a project with all its repos and worktrees.
    pub async fn get_project_with_relations(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectWithRelations>> {
        let Some(project) = self.get_project(project_id).await? else {
            return Ok(None);
        };
        let repos = self.list_repos(project_id).await?;
        let worktrees = self.list_worktrees_with_repos(project_id).await?;
        Ok(Some(ProjectWithRelations {
            project,
            repos,
            worktrees,
        }))
    }

    // -- Repos --------------------------------------------------------------

    pub async fn create_repo(&self, input: CreateRepoInput) -> Result<ProjectRepo> {
        let id = uuid::Uuid::new_v4().to_string();

        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                r#"
                INSERT INTO project_repos (id, project_id, name, path, remote_url, default_branch, current_branch, description)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&input.project_id)
            .bind(&input.name)
            .bind(&input.path)
            .bind(&input.remote_url)
            .bind(&input.default_branch)
            .bind(&input.current_branch)
            .bind(&input.description)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to insert repo")?,
            DbPool::Postgres(p) => sqlx::query(
                r#"
                INSERT INTO project_repos (id, project_id, name, path, remote_url, default_branch, current_branch, description)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(&id)
            .bind(&input.project_id)
            .bind(&input.name)
            .bind(&input.path)
            .bind(&input.remote_url)
            .bind(&input.default_branch)
            .bind(&input.current_branch)
            .bind(&input.description)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to insert repo")?,
        };

        Ok(self
            .get_repo(&id)
            .await?
            .context("repo not found after insert")?)
    }

    pub async fn get_repo(&self, repo_id: &str) -> Result<Option<ProjectRepo>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query("SELECT * FROM project_repos WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .context("failed to fetch repo")?
                .map(|r| row_to_repo_sqlite(&r))
                .transpose(),
            DbPool::Postgres(p) => sqlx::query("SELECT * FROM project_repos WHERE id = $1")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .context("failed to fetch repo")?
                .map(|r| row_to_repo_pg(&r))
                .transpose(),
        }
    }

    pub async fn list_repos(&self, project_id: &str) -> Result<Vec<ProjectRepo>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                "SELECT * FROM project_repos WHERE project_id = ? ORDER BY name ASC",
            )
            .bind(project_id)
            .fetch_all(p)
            .await
            .context("failed to list repos")?
            .iter()
            .map(row_to_repo_sqlite)
            .collect(),
            DbPool::Postgres(p) => sqlx::query(
                "SELECT * FROM project_repos WHERE project_id = $1 ORDER BY name ASC",
            )
            .bind(project_id)
            .fetch_all(p)
            .await
            .context("failed to list repos")?
            .iter()
            .map(row_to_repo_pg)
            .collect(),
        }
    }

    pub async fn delete_repo(&self, repo_id: &str) -> Result<bool> {
        let rows_affected = match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query("DELETE FROM project_repos WHERE id = ?")
                .bind(repo_id)
                .execute(p)
                .await
                .context("failed to delete repo")?
                .rows_affected(),
            DbPool::Postgres(p) => sqlx::query("DELETE FROM project_repos WHERE id = $1")
                .bind(repo_id)
                .execute(p)
                .await
                .context("failed to delete repo")?
                .rows_affected(),
        };
        Ok(rows_affected > 0)
    }

    /// Find a repo by its relative path within a project.
    pub async fn get_repo_by_path(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<Option<ProjectRepo>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                "SELECT * FROM project_repos WHERE project_id = ? AND path = ?",
            )
            .bind(project_id)
            .bind(path)
            .fetch_optional(p)
            .await
            .context("failed to fetch repo by path")?
            .map(|r| row_to_repo_sqlite(&r))
            .transpose(),
            DbPool::Postgres(p) => sqlx::query(
                "SELECT * FROM project_repos WHERE project_id = $1 AND path = $2",
            )
            .bind(project_id)
            .bind(path)
            .fetch_optional(p)
            .await
            .context("failed to fetch repo by path")?
            .map(|r| row_to_repo_pg(&r))
            .transpose(),
        }
    }

    // -- Worktrees ----------------------------------------------------------

    pub async fn create_worktree(&self, input: CreateWorktreeInput) -> Result<ProjectWorktree> {
        let id = uuid::Uuid::new_v4().to_string();

        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                r#"
                INSERT INTO project_worktrees (id, project_id, repo_id, name, path, branch, created_by)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&input.project_id)
            .bind(&input.repo_id)
            .bind(&input.name)
            .bind(&input.path)
            .bind(&input.branch)
            .bind(&input.created_by)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to insert worktree")?,
            DbPool::Postgres(p) => sqlx::query(
                r#"
                INSERT INTO project_worktrees (id, project_id, repo_id, name, path, branch, created_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(&id)
            .bind(&input.project_id)
            .bind(&input.repo_id)
            .bind(&input.name)
            .bind(&input.path)
            .bind(&input.branch)
            .bind(&input.created_by)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to insert worktree")?,
        };

        Ok(self
            .get_worktree(&id)
            .await?
            .context("worktree not found after insert")?)
    }

    pub async fn get_worktree(&self, worktree_id: &str) -> Result<Option<ProjectWorktree>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query("SELECT * FROM project_worktrees WHERE id = ?")
                .bind(worktree_id)
                .fetch_optional(p)
                .await
                .context("failed to fetch worktree")?
                .map(|r| row_to_worktree_sqlite(&r))
                .transpose(),
            DbPool::Postgres(p) => sqlx::query("SELECT * FROM project_worktrees WHERE id = $1")
                .bind(worktree_id)
                .fetch_optional(p)
                .await
                .context("failed to fetch worktree")?
                .map(|r| row_to_worktree_pg(&r))
                .transpose(),
        }
    }

    pub async fn list_worktrees(&self, project_id: &str) -> Result<Vec<ProjectWorktree>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                "SELECT * FROM project_worktrees WHERE project_id = ? ORDER BY name ASC",
            )
            .bind(project_id)
            .fetch_all(p)
            .await
            .context("failed to list worktrees")?
            .iter()
            .map(row_to_worktree_sqlite)
            .collect(),
            DbPool::Postgres(p) => sqlx::query(
                "SELECT * FROM project_worktrees WHERE project_id = $1 ORDER BY name ASC",
            )
            .bind(project_id)
            .fetch_all(p)
            .await
            .context("failed to list worktrees")?
            .iter()
            .map(row_to_worktree_pg)
            .collect(),
        }
    }

    /// List worktrees with the source repo name resolved via JOIN.
    pub async fn list_worktrees_with_repos(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectWorktreeWithRepo>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                r#"
                SELECT w.*, r.name AS repo_name
                FROM project_worktrees w
                JOIN project_repos r ON w.repo_id = r.id
                WHERE w.project_id = ?
                ORDER BY w.name ASC
                "#,
            )
            .bind(project_id)
            .fetch_all(p)
            .await
            .context("failed to list worktrees with repos")?
            .iter()
            .map(|r| {
                let worktree = row_to_worktree_sqlite(r)?;
                let repo_name: String =
                    r.try_get("repo_name").context("missing repo_name")?;
                Ok(ProjectWorktreeWithRepo { worktree, repo_name })
            })
            .collect(),
            DbPool::Postgres(p) => sqlx::query(
                r#"
                SELECT w.*, r.name AS repo_name
                FROM project_worktrees w
                JOIN project_repos r ON w.repo_id = r.id
                WHERE w.project_id = $1
                ORDER BY w.name ASC
                "#,
            )
            .bind(project_id)
            .fetch_all(p)
            .await
            .context("failed to list worktrees with repos")?
            .iter()
            .map(|r| {
                let worktree = row_to_worktree_pg(r)?;
                let repo_name: String =
                    r.try_get("repo_name").context("missing repo_name")?;
                Ok(ProjectWorktreeWithRepo { worktree, repo_name })
            })
            .collect(),
        }
    }

    pub async fn delete_worktree(&self, worktree_id: &str) -> Result<bool> {
        let rows_affected = match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query("DELETE FROM project_worktrees WHERE id = ?")
                .bind(worktree_id)
                .execute(p)
                .await
                .context("failed to delete worktree")?
                .rows_affected(),
            DbPool::Postgres(p) => sqlx::query("DELETE FROM project_worktrees WHERE id = $1")
                .bind(worktree_id)
                .execute(p)
                .await
                .context("failed to delete worktree")?
                .rows_affected(),
        };
        Ok(rows_affected > 0)
    }

    /// Find a worktree by its relative path within a project.
    pub async fn get_worktree_by_path(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<Option<ProjectWorktree>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                "SELECT * FROM project_worktrees WHERE project_id = ? AND path = ?",
            )
            .bind(project_id)
            .bind(path)
            .fetch_optional(p)
            .await
            .context("failed to fetch worktree by path")?
            .map(|r| row_to_worktree_sqlite(&r))
            .transpose(),
            DbPool::Postgres(p) => sqlx::query(
                "SELECT * FROM project_worktrees WHERE project_id = $1 AND path = $2",
            )
            .bind(project_id)
            .bind(path)
            .fetch_optional(p)
            .await
            .context("failed to fetch worktree by path")?
            .map(|r| row_to_worktree_pg(&r))
            .transpose(),
        }
    }

    /// Update the current_branch for a repo (e.g. after a scan detects a checkout change).
    pub async fn update_repo_current_branch(
        &self,
        repo_id: &str,
        current_branch: Option<&str>,
    ) -> Result<()> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                "UPDATE project_repos SET current_branch = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(current_branch)
            .bind(repo_id)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to update repo current_branch")?,
            DbPool::Postgres(p) => sqlx::query(
                "UPDATE project_repos SET current_branch = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
            )
            .bind(current_branch)
            .bind(repo_id)
            .execute(p)
            .await
            .map(|_| ())
            .context("failed to update repo current_branch")?,
        };
        Ok(())
    }

    /// Update cached disk usage for a repo.
    pub async fn set_repo_disk_usage(&self, repo_id: &str, bytes: i64) -> Result<()> {
        match &*self.pool {
            DbPool::Sqlite(p) => {
                sqlx::query("UPDATE project_repos SET disk_usage_bytes = ? WHERE id = ?")
                    .bind(bytes)
                    .bind(repo_id)
                    .execute(p)
                    .await
                    .context("failed to update repo disk usage")?;
            }
            DbPool::Postgres(p) => {
                sqlx::query("UPDATE project_repos SET disk_usage_bytes = $1 WHERE id = $2")
                    .bind(bytes)
                    .bind(repo_id)
                    .execute(p)
                    .await
                    .context("failed to update repo disk usage")?;
            }
        }
        Ok(())
    }

    /// Update cached disk usage for a worktree.
    pub async fn set_worktree_disk_usage(&self, worktree_id: &str, bytes: i64) -> Result<()> {
        match &*self.pool {
            DbPool::Sqlite(p) => {
                sqlx::query("UPDATE project_worktrees SET disk_usage_bytes = ? WHERE id = ?")
                    .bind(bytes)
                    .bind(worktree_id)
                    .execute(p)
                    .await
                    .context("failed to update worktree disk usage")?;
            }
            DbPool::Postgres(p) => {
                sqlx::query("UPDATE project_worktrees SET disk_usage_bytes = $1 WHERE id = $2")
                    .bind(bytes)
                    .bind(worktree_id)
                    .execute(p)
                    .await
                    .context("failed to update worktree disk usage")?;
            }
        }
        Ok(())
    }

    /// Set the detected logo path for a project.
    pub async fn set_logo_path(&self, project_id: &str, logo_path: Option<&str>) -> Result<()> {
        match &*self.pool {
            DbPool::Sqlite(p) => {
                sqlx::query(
                    "UPDATE projects SET logo_path = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(logo_path)
                .bind(project_id)
                .execute(p)
                .await
                .context("failed to update project logo_path")?;
            }
            DbPool::Postgres(p) => {
                sqlx::query(
                    "UPDATE projects SET logo_path = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
                )
                .bind(logo_path)
                .bind(project_id)
                .execute(p)
                .await
                .context("failed to update project logo_path")?;
            }
        }
        Ok(())
    }

    /// List worktrees belonging to a specific repo.
    pub async fn list_worktrees_for_repo(&self, repo_id: &str) -> Result<Vec<ProjectWorktree>> {
        match &*self.pool {
            DbPool::Sqlite(p) => sqlx::query(
                "SELECT * FROM project_worktrees WHERE repo_id = ? ORDER BY name ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .context("failed to list worktrees for repo")?
            .iter()
            .map(row_to_worktree_sqlite)
            .collect(),
            DbPool::Postgres(p) => sqlx::query(
                "SELECT * FROM project_worktrees WHERE repo_id = $1 ORDER BY name ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .context("failed to list worktrees for repo")?
            .iter()
            .map(row_to_worktree_pg)
            .collect(),
        }
    }
}

// Row mapping helpers — split per backend because the schema diverges:
// SQLite uses DATETIME columns (read as String directly), Postgres uses
// TIMESTAMPTZ columns (read as chrono::DateTime<Utc>, formatted to RFC-3339).

fn row_to_project_sqlite(row: &sqlx::sqlite::SqliteRow) -> Result<Project> {
    let tags_raw: String = row.try_get("tags").context("missing tags")?;
    let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();

    let settings_raw: String = row.try_get("settings").context("missing settings")?;
    let settings: Value =
        serde_json::from_str(&settings_raw).unwrap_or(Value::Object(Default::default()));

    let status_raw: String = row.try_get("status").context("missing status")?;
    let status = ProjectStatus::parse(&status_raw).unwrap_or(ProjectStatus::Active);

    Ok(Project {
        id: row.try_get("id").context("missing id")?,
        name: row.try_get("name").context("missing name")?,
        description: row.try_get("description").context("missing description")?,
        icon: row.try_get("icon").context("missing icon")?,
        tags,
        root_path: row.try_get("root_path").context("missing root_path")?,
        logo_path: row.try_get("logo_path").unwrap_or(None),
        settings,
        status,
        sort_order: row.try_get("sort_order").unwrap_or(0),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn row_to_project_pg(row: &sqlx::postgres::PgRow) -> Result<Project> {
    let tags_raw: String = row.try_get("tags").context("missing tags")?;
    let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();

    let settings_raw: String = row.try_get("settings").context("missing settings")?;
    let settings: Value =
        serde_json::from_str(&settings_raw).unwrap_or(Value::Object(Default::default()));

    let status_raw: String = row.try_get("status").context("missing status")?;
    let status = ProjectStatus::parse(&status_raw).unwrap_or(ProjectStatus::Active);

    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").context("missing created_at")?;
    let updated_at: chrono::DateTime<chrono::Utc> =
        row.try_get("updated_at").context("missing updated_at")?;

    Ok(Project {
        id: row.try_get("id").context("missing id")?,
        name: row.try_get("name").context("missing name")?,
        description: row.try_get("description").context("missing description")?,
        icon: row.try_get("icon").context("missing icon")?,
        tags,
        root_path: row.try_get("root_path").context("missing root_path")?,
        logo_path: row.try_get("logo_path").unwrap_or(None),
        settings,
        status,
        sort_order: row.try_get::<i32, _>("sort_order").unwrap_or(0) as i64,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    })
}

fn row_to_repo_sqlite(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectRepo> {
    Ok(ProjectRepo {
        id: row.try_get("id").context("missing id")?,
        project_id: row.try_get("project_id").context("missing project_id")?,
        name: row.try_get("name").context("missing name")?,
        path: row.try_get("path").context("missing path")?,
        remote_url: row.try_get("remote_url").context("missing remote_url")?,
        default_branch: row
            .try_get("default_branch")
            .context("missing default_branch")?,
        current_branch: row.try_get("current_branch").unwrap_or(None),
        description: row.try_get("description").context("missing description")?,
        disk_usage_bytes: row.try_get("disk_usage_bytes").unwrap_or(None),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn row_to_repo_pg(row: &sqlx::postgres::PgRow) -> Result<ProjectRepo> {
    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").context("missing created_at")?;
    let updated_at: chrono::DateTime<chrono::Utc> =
        row.try_get("updated_at").context("missing updated_at")?;
    Ok(ProjectRepo {
        id: row.try_get("id").context("missing id")?,
        project_id: row.try_get("project_id").context("missing project_id")?,
        name: row.try_get("name").context("missing name")?,
        path: row.try_get("path").context("missing path")?,
        remote_url: row.try_get("remote_url").context("missing remote_url")?,
        default_branch: row
            .try_get("default_branch")
            .context("missing default_branch")?,
        current_branch: row.try_get("current_branch").unwrap_or(None),
        description: row.try_get("description").context("missing description")?,
        disk_usage_bytes: row.try_get("disk_usage_bytes").unwrap_or(None),
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    })
}

fn row_to_worktree_sqlite(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectWorktree> {
    Ok(ProjectWorktree {
        id: row.try_get("id").context("missing id")?,
        project_id: row.try_get("project_id").context("missing project_id")?,
        repo_id: row.try_get("repo_id").context("missing repo_id")?,
        name: row.try_get("name").context("missing name")?,
        path: row.try_get("path").context("missing path")?,
        branch: row.try_get("branch").context("missing branch")?,
        created_by: row.try_get("created_by").context("missing created_by")?,
        disk_usage_bytes: row.try_get("disk_usage_bytes").unwrap_or(None),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn row_to_worktree_pg(row: &sqlx::postgres::PgRow) -> Result<ProjectWorktree> {
    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").context("missing created_at")?;
    let updated_at: chrono::DateTime<chrono::Utc> =
        row.try_get("updated_at").context("missing updated_at")?;
    Ok(ProjectWorktree {
        id: row.try_get("id").context("missing id")?,
        project_id: row.try_get("project_id").context("missing project_id")?,
        repo_id: row.try_get("repo_id").context("missing repo_id")?,
        name: row.try_get("name").context("missing name")?,
        path: row.try_get("path").context("missing path")?,
        branch: row.try_get("branch").context("missing branch")?,
        created_by: row.try_get("created_by").context("missing created_by")?,
        disk_usage_bytes: row.try_get("disk_usage_bytes").unwrap_or(None),
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    })
}

// Logo detection

/// File names we recognise as a project logo, in priority order.
const LOGO_NAMES: &[&str] = &[
    "logo.png",
    "logo.svg",
    "logo.webp",
    "icon.png",
    "icon.svg",
    "icon.ico",
    "favicon.png",
    "favicon.ico",
    "favicon.svg",
];

/// Detect a project logo by scanning for well-known file names up to 3 levels
/// deep. Returns the relative path (relative to project root) of the first
/// match. Skips hidden directories (except `.github`), `node_modules`, and
/// `target` to keep it fast.
pub fn detect_logo(root: &Path) -> Option<String> {
    scan_for_logo(root, root, 0)
}

fn scan_for_logo(root: &Path, dir: &Path, depth: u8) -> Option<String> {
    if depth > 3 {
        return None;
    }

    // Check for logo files in this directory
    for name in LOGO_NAMES {
        let candidate = dir.join(name);
        if candidate.is_file() {
            let rel = candidate.strip_prefix(root).ok()?;
            return Some(rel.to_string_lossy().to_string());
        }
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return None,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Skip heavy/irrelevant dirs, but allow .github
        if name == "node_modules"
            || name == "target"
            || name == ".git"
            || name == "dist"
            || name == "build"
        {
            continue;
        }
        if name.starts_with('.') && name != ".github" {
            continue;
        }
        if let Some(found) = scan_for_logo(root, &path, depth + 1) {
            return Some(found);
        }
    }

    None
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_pool() -> Arc<DbPool> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("failed to create in-memory pool");
        sqlx::migrate!("./migrations/global")
            .run(&pool)
            .await
            .expect("failed to run migrations");
        Arc::new(DbPool::Sqlite(pool))
    }

    #[tokio::test]
    async fn create_and_list_project() {
        let pool = setup_pool().await;
        let store = ProjectStore::new(pool);

        let project = store
            .create_project(CreateProjectInput {
                name: "Spacebot".into(),
                description: "The Spacebot monorepo".into(),
                icon: "".into(),
                tags: vec!["rust".into(), "agent".into()],
                root_path: "/home/user/Projects/spacebot".into(),
                settings: Value::Object(Default::default()),
            })
            .await
            .expect("failed to create project");

        assert_eq!(project.name, "Spacebot");
        assert_eq!(project.tags, vec!["rust", "agent"]);
        assert_eq!(project.status, ProjectStatus::Active);

        let projects = store
            .list_projects(None)
            .await
            .expect("failed to list projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, project.id);
    }

    #[tokio::test]
    async fn create_repo_and_worktree() {
        let pool = setup_pool().await;
        let store = ProjectStore::new(pool);

        let project = store
            .create_project(CreateProjectInput {
                name: "Test".into(),
                description: String::new(),
                icon: String::new(),
                tags: vec![],
                root_path: "/tmp/test-project".into(),
                settings: Value::Object(Default::default()),
            })
            .await
            .expect("failed to create project");

        let repo = store
            .create_repo(CreateRepoInput {
                project_id: project.id.clone(),
                name: "spacebot".into(),
                path: "spacebot".into(),
                remote_url: "https://github.com/spacedriveapp/spacebot.git".into(),
                default_branch: "main".into(),
                current_branch: Some("feat/projects".into()),
                description: "Core agent".into(),
            })
            .await
            .expect("failed to create repo");

        assert_eq!(repo.name, "spacebot");

        let worktree = store
            .create_worktree(CreateWorktreeInput {
                project_id: project.id.clone(),
                repo_id: repo.id.clone(),
                name: "feat-projects".into(),
                path: "feat-projects".into(),
                branch: "feat/projects".into(),
                created_by: "user".into(),
            })
            .await
            .expect("failed to create worktree");

        assert_eq!(worktree.branch, "feat/projects");
        assert_eq!(worktree.created_by, "user");

        let with_repos = store
            .list_worktrees_with_repos(&project.id)
            .await
            .expect("failed to list worktrees with repos");
        assert_eq!(with_repos.len(), 1);
        assert_eq!(with_repos[0].repo_name, "spacebot");
    }

    #[tokio::test]
    async fn update_project_status() {
        let pool = setup_pool().await;
        let store = ProjectStore::new(pool);

        let project = store
            .create_project(CreateProjectInput {
                name: "Test".into(),
                description: String::new(),
                icon: String::new(),
                tags: vec![],
                root_path: "/tmp/test".into(),
                settings: Value::Object(Default::default()),
            })
            .await
            .expect("failed to create project");

        let updated = store
            .update_project(
                &project.id,
                UpdateProjectInput {
                    status: Some(ProjectStatus::Archived),
                    ..Default::default()
                },
            )
            .await
            .expect("failed to update project")
            .expect("project not found");

        assert_eq!(updated.status, ProjectStatus::Archived);

        // Filtering by active should return empty.
        let active = store
            .list_projects(Some(ProjectStatus::Active))
            .await
            .expect("failed to list");
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn delete_project_cascades() {
        let pool = setup_pool().await;
        let store = ProjectStore::new(pool);

        let project = store
            .create_project(CreateProjectInput {
                name: "Test".into(),
                description: String::new(),
                icon: String::new(),
                tags: vec![],
                root_path: "/tmp/cascade-test".into(),
                settings: Value::Object(Default::default()),
            })
            .await
            .expect("failed to create project");

        let repo = store
            .create_repo(CreateRepoInput {
                project_id: project.id.clone(),
                name: "repo".into(),
                path: "repo".into(),
                remote_url: String::new(),
                default_branch: "main".into(),
                current_branch: None,
                description: String::new(),
            })
            .await
            .expect("failed to create repo");

        store
            .create_worktree(CreateWorktreeInput {
                project_id: project.id.clone(),
                repo_id: repo.id.clone(),
                name: "wt".into(),
                path: "wt".into(),
                branch: "feat/x".into(),
                created_by: "agent".into(),
            })
            .await
            .expect("failed to create worktree");

        let deleted = store
            .delete_project(&project.id)
            .await
            .expect("failed to delete project");
        assert!(deleted);

        // Repos and worktrees should be gone via CASCADE.
        let repos = store
            .list_repos(&project.id)
            .await
            .expect("failed to list repos");
        assert!(repos.is_empty());

        let worktrees = store
            .list_worktrees(&project.id)
            .await
            .expect("failed to list worktrees");
        assert!(worktrees.is_empty());
    }

    #[tokio::test]
    async fn duplicate_root_path_rejected() {
        let pool = setup_pool().await;
        let store = ProjectStore::new(pool);

        store
            .create_project(CreateProjectInput {
                name: "First".into(),
                description: String::new(),
                icon: String::new(),
                tags: vec![],
                root_path: "/tmp/unique-path".into(),
                settings: Value::Object(Default::default()),
            })
            .await
            .expect("failed to create first project");

        let result = store
            .create_project(CreateProjectInput {
                name: "Second".into(),
                description: String::new(),
                icon: String::new(),
                tags: vec![],
                root_path: "/tmp/unique-path".into(),
                settings: Value::Object(Default::default()),
            })
            .await;

        assert!(result.is_err(), "duplicate root_path should fail");
    }
}
