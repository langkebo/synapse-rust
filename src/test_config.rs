//! Test configuration utilities
//!
//! Provides centralized configuration for test environments,
//! eliminating hardcoded connection strings and paths.
//!
//! This module re-exports the canonical implementation from
//! `synapse_services::test_config` to avoid duplication.
//! See ROUND2-ISSUE-2 for the env-var synchronization fix.

#![cfg(feature = "test-utils")]

pub use synapse_services::test_config::*;
