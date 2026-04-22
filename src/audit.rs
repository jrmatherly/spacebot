//! Audit subsystem. Hash-chained append-only log for SOC 2 CC7.2 evidence.

pub mod appender;
pub mod export;
pub mod types;

pub use appender::AuditAppender;
pub use types::{AuditAction, AuditEvent, AuditRow};
