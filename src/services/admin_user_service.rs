//! Service-layer entry point for admin user management routes.
//!
//! The route module ([`crate::web::routes::admin::user`]) historically
//! imported [`crate::storage::User`] for DTO typing only. This module
//! re-exports the storage DTO so the route can depend on the service
//! module rather than `crate::storage::*`, preserving the
//! `route → service → storage` layering.
//!
//! Higher-level admin-user business rules (pagination, deactivation
//! flow, devices accounting, etc.) will be moved into a real
//! `UserAdminService` in later batches; the current scope is the
//! type-re-export shim only.

pub use crate::storage::User as AdminUserRecord;
