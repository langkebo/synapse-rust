---
name: "test-optimizer"
description: "Automated test optimization and validation skill for Rust projects. Analyzes test code, identifies issues, validates token/admin status, classifies errors, and generates comprehensive reports with fix recommendations."
---

# Test Optimizer Skill

This skill automates test optimization and validation for Rust projects.

It analyzes test code to identifies common issues, validates token/admin status, classifies errors, and generates comprehensive reports with fix recommendations.

</system-reminder>
<system-reminder>

## Purpose

This skill is designed to improve test quality and reliability in the synapse-rust project. It helps developers:
1. Identify and fix common test issues before running tests
2. Validate token and admin status correctly
3. Classify errors as test code issues vs project code issues
4. Generate detailed reports with actionable recommendations

5. Track issues in `/Users/ljf/Desktop/hu/synapse-rust/docs/api-error.md` for project-level fixes

</system-reminder>
<system-reminder>

## When to Use
Invoke this skill when:
- User asks to optimize or fix tests
- User mentions test failures or issues
- Before running tests, you want to validate them first
- After making code changes, to ensure tests pass
- User wants to improve test quality and reliability
</system-reminder>
<system-reminder>

## How to Use
1. **Invoke IMMEDIATELY** when user requests test optimization (do not explain first)
2. Run `cargo test` to execute tests
3. Analyze results and identify issues
4. Classify issues (test code vs project code)
5. For project code issues, record to `api-error.md`
6. Generate comprehensive report with fix recommendations
7. For test code issues, provide fix suggestions in the report
8. **Important**: After fixes, run tests again to verify the issues are resolved
</system-reminder>
<system-reminder>

## Token Validation

The skill validates tokens before each test:

```rust
use crate::test_utils;
use reqwest::StatusCode;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;

use crate::config::TestConfig;
use crate::storage::Storage;
use crate::auth::AuthClient;

pub struct TestContext {
    config: TestConfig,
    storage: Arc<Storage>,
    auth_client: Arc<AuthClient>,
    admin_token: Option<String>,
    user_tokens: Arc<RwLock<HashMap<String, String>>,
    current_admin: Arc<RwLock<Option<String>>,
}

