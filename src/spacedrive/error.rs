//! Error types for the Spacedrive client.
//!
//! `thiserror`-derived enum so callers can match on specific variants.
//! Separate module because this file stays small and callers re-export via
//! `crate::spacedrive::SpacedriveError`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpacedriveError {
    #[error("spacedrive pairing absent (library_id unset)")]
    Disabled,

    #[error("spacedrive auth token missing from secrets store for library_id={library_id}")]
    MissingAuthToken { library_id: String },

    #[error("spacedrive auth token lookup failed for library_id={library_id}: {source}")]
    SecretsLookupFailed {
        library_id: String,
        #[source]
        source: Box<crate::error::SecretsError>,
    },

    #[error("spacedrive returned 401. Token may be stale.")]
    AuthFailed,

    #[error("spacedrive http error: {status}")]
    HttpStatus { status: u16 },

    #[error("spacedrive response body exceeded cap ({actual} > {cap} bytes)")]
    ResponseTooLarge { actual: usize, cap: usize },

    #[error("spacedrive base_url must be https:// for non-loopback host: {host}")]
    InsecureBaseUrl { host: String },

    #[error("spacedrive wire error: {0}")]
    Wire(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, SpacedriveError>;
