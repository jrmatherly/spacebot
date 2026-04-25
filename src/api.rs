//! HTTP API server for the Spacebot control interface.
//!
//! Serves the embedded frontend assets and provides a JSON API for
//! managing agents, viewing status, and interacting with the system.
//! Includes an SSE endpoint for realtime event streaming.

mod activity;
mod admin_access_review;
mod admin_claim;
mod admin_teams;
pub mod agents;
mod attachments;
mod audit;
mod auth_config;
mod bindings;
mod channels;
mod config;
mod cortex;
mod cron;
mod desktop;
mod factory;
mod ingest;
mod links;
mod mcp;
mod me;
mod memories;
mod messaging;
mod models;
mod notifications;
mod opencode_proxy;
mod portal;
mod projects;
pub(crate) mod providers;
mod resources;
mod secrets;
mod server;
mod settings;
mod skills;
pub(crate) mod ssh;
mod state;
mod system;
mod tasks;
mod tools;
mod usage;
mod wiki;
mod workers;

pub use server::{api_router, start_http_server};
pub use state::{AgentInfo, ApiEvent, ApiState, ChannelToolCallEntry};

#[doc(hidden)]
pub use server::test_support;
