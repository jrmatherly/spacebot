//! Record types for the Phase 2 authz data model. Kept 1:1 with SQL tables
//! so `sqlx::FromRow` derivation is trivial. Business logic lives in
//! `repository.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserRecord {
    pub principal_key: String,
    pub tenant_id: String,
    pub object_id: String,
    pub principal_type: String,
    pub display_name: Option<String>,
    pub display_email: Option<String>,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TeamRecord {
    pub id: String,
    pub external_id: String,
    pub display_name: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TeamMembershipRecord {
    pub principal_key: String,
    pub team_id: String,
    pub observed_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceAccountRecord {
    pub principal_key: String,
    pub description: String,
    pub owner_principal_key: String,
    pub assigned_roles_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Personal,
    Team,
    Org,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Personal => "personal",
            Visibility::Team => "team",
            Visibility::Org => "org",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "personal" => Some(Self::Personal),
            "team" => Some(Self::Team),
            "org" => Some(Self::Org),
            _ => None,
        }
    }
}

/// Query-param scope for list endpoints that support narrowing results to
/// "resources the caller owns" / "resources in the caller's teams" / "the
/// full org view." Distinct from [`Visibility`], which is the persisted
/// property on each resource row. `ResourceScope` is the query-time lens
/// over ownership; `Visibility` is the storage-time classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResourceScope {
    Mine,
    Team,
    Org,
}

impl ResourceScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceScope::Mine => "mine",
            ResourceScope::Team => "team",
            ResourceScope::Org => "org",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mine" => Some(Self::Mine),
            "team" => Some(Self::Team),
            "org" => Some(Self::Org),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResourceOwnershipRecord {
    pub resource_type: String,
    pub resource_id: String,
    pub owner_agent_id: Option<String>,
    pub owner_principal_key: String,
    pub visibility: String,
    pub shared_with_team_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ResourceOwnershipRecord {
    pub fn visibility_enum(&self) -> Option<Visibility> {
        Visibility::parse(&self.visibility)
    }
}
