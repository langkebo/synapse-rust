# synapse-rust 测试接线与执行清单

> 目的：确认“仓库里有哪些测试”与“实际会执行哪些测试”之间的差异  
> 判定规则：仅当测试文件被 `Cargo.toml` 声明的测试入口、`tests/` 根目录自动发现入口，或被入口 `mod` 引入时，才视为已接线

---

## 一、入口基线

当前仓库存在 4 个实际测试入口：

| 类型 | 入口 | 证据 |
|------|------|------|
| 显式测试入口 | `unit` | [Cargo.toml:L153-L155](file:///Users/ljf/Desktop/hu/synapse-rust/Cargo.toml#L153-L155) |
| 显式测试入口 | `integration` | [Cargo.toml:L149-L151](file:///Users/ljf/Desktop/hu/synapse-rust/Cargo.toml#L149-L151) |
| 显式测试入口 | `e2e` | [Cargo.toml](file:///Users/ljf/Desktop/hu/synapse-rust/Cargo.toml) |
| 根目录自动发现 | `tests/friend_federation_test.rs` | [friend_federation_test.rs:L5-L6](file:///Users/ljf/Desktop/hu/synapse-rust/tests/friend_federation_test.rs#L5-L6) |

补充说明：

- `tests/unit/` 是否执行，取决于 [unit/mod.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/unit/mod.rs)
- `tests/integration/` 是否执行，取决于 [integration/mod.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/mod.rs)
- `tests/e2e/` 已有独立入口，`tests/performance/`、`tests/example/` 当前仍没有独立入口

---

## 二、目录级结论

| 目录 | 状态 | 结论 |
|------|------|------|
| `tests/unit/` | 部分接线 | 由 `tests/unit/mod.rs` 显式引入的文件会执行，未引入文件不会执行 |
| `tests/integration/` | 部分接线 | 由 `tests/integration/mod.rs` 显式引入的文件会执行，未引入文件不会执行 |
| `tests/e2e/` | 已接线 | 目录已在 `Cargo.toml` 中注册为独立 `e2e` 测试入口 |
| `tests/performance/` | 未接线 | 目录存在但未在 `Cargo.toml` 中注册 |
| `tests/example/` | 未接线 | 子目录文件不会被自动发现 |
| `tests/common/` | 辅助模块部分接线 | 只有 `common/mod.rs` 被引入，其他辅助文件未显式接线 |
| `tests/` 根目录 | 部分接线 | `friend_federation_test.rs` 自动发现；`api_tests.json` 不是 Rust 测试入口 |

---

## 三、已接线测试

### 3.1 根目录自动发现

- [friend_federation_test.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/friend_federation_test.rs)

### 3.2 `unit` 入口已接线

证据：[unit/mod.rs:L4-L50](file:///Users/ljf/Desktop/hu/synapse-rust/tests/unit/mod.rs#L4-L50)

- `admin_api_tests.rs`
- `admin_extra_api_tests.rs`
- `app_service_api_tests.rs`
- `auth_service_tests.rs`
- `background_update_api_tests.rs`
- `boundary_tests.rs`
- `captcha_api_tests.rs`
- `core_api_tests.rs`
- `db_schema_smoke_tests.rs`
- `e2ee_api_tests.rs`
- `event_report_api_tests.rs`
- `federation_api_tests.rs`
- `federation_cache_api_tests.rs`
- `friend_api_tests.rs`
- `invite_blocklist_tests.rs`
- `key_backup_api_tests.rs`
- `media_api_tests.rs`
- `media_quota_api_tests.rs`
- `module_api_tests.rs`
- `msc_tests.rs`
- `push_api_tests.rs`
- `rate_limit_api_tests.rs`
- `reactions_api_tests.rs`
- `refresh_token_api_tests.rs`
- `registration_token_api_tests.rs`
- `retention_api_tests.rs`
- `retention_storage_tests.rs`
- `room_summary_api_tests.rs`
- `room_summary_storage_tests.rs`
- `room_service_tests.rs`
- `search_service_tests.rs`
- `server_notification_api_tests.rs`
- `sliding_sync_api_tests.rs`
- `space_api_tests.rs`
- `sync_service_tests.rs`
- `telemetry_api_tests.rs`
- `thread_api_tests.rs`
- `thread_storage_tests.rs`
- `worker_api_tests.rs`
- `directory_service_tests.rs`
- `dm_service_tests.rs`
- `typing_service_tests.rs`
- `coverage_tests.rs`
- `worker_coverage_tests.rs`
- `../common/mod.rs`

### 3.3 `integration` 入口已接线

证据：[integration/mod.rs:L1-L37](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/mod.rs#L1-L37)

- `api_account_data_routes_tests.rs`
- `api_admin_audit_tests.rs`
- `api_admin_federation_tests.rs`
- `api_admin_regression_tests.rs`
- `api_admin_tests.rs`
- `api_auth_routes_tests.rs`
- `api_device_presence_tests.rs`
- `api_device_routes_tests.rs`
- `api_e2ee_tests.rs`
- `api_enhanced_features_tests.rs`
- `api_feature_flags_tests.rs`
- `api_federation_tests.rs`
- `api_friend_room_routes_tests.rs`
- `api_input_validation_tests.rs`
- `api_ip_block_test.rs`
- `api_media_routes_tests.rs`
- `api_profile_tests.rs`
- `api_protocol_alignment_tests.rs`
- `api_room_summary_routes_tests.rs`
- `api_room_tests.rs`
- `api_search_thread_tests.rs`
- `api_telemetry_alerts_tests.rs`
- `api_widget_tests.rs`
- `cache_tests.rs`
- `concurrency_tests.rs`
- `metrics_tests.rs`
- `federation_error_tests.rs`
- `missing_features_tests.rs`
- `password_hash_pool_tests.rs`
- `protocol_compliance_tests.rs`
- `regex_cache_tests.rs`
- `transaction_tests.rs`
- `voice_routes_tests.rs`
- `coverage_tests.rs`
- `schema_validation_tests.rs`

---

## 四、未接线测试

### 4.1 `tests/unit/` 未被 `unit/mod.rs` 引入

- `application_service_tests.rs`
- `background_update_tests.rs`
- `captcha_tests.rs`
- `device_storage_tests.rs`
- `event_report_tests.rs`
- `event_storage_tests.rs`
- `exception_tests.rs`
- `federation_service_tests.rs`
- `federation_signature_cache_tests.rs`
- `friend_groups_tests.rs`
- `matrixrtc_tests.rs`
- `media_service_tests.rs`
- `module_tests.rs`
- `new_features_tests.rs`
- `pool_monitor_tests.rs`
- `qr_login_tests.rs`
- `rate_limit_config_tests.rs`
- `refresh_token_tests.rs`
- `registration_service_tests.rs`
- `registration_token_tests.rs`
- `retention_tests.rs`
- `room_cache_tests.rs`
- `room_summary_tests.rs`
- `saml_tests.rs`
- `security_tests.rs`
- `service_tests.rs`
- `sticky_event_tests.rs`
- `storage_tests.rs`
- `voice_service_tests.rs`
- `worker_tests.rs`

### 4.2 `tests/integration/` 未被 `integration/mod.rs` 引入

- 当前无

### 4.3 `tests/e2e/` 已接线

证据：`Cargo.toml` 已声明对应 `[[test]]`

- `mod.rs`
- `e2e_scenarios.rs`
- `user_flow_tests.rs`

### 4.4 `tests/performance/` 整组未接线

证据：`Cargo.toml` 未声明对应 `[[test]]`

- `mod.rs`
- `api_load_tests.rs`
- `benchmarks.rs`
- `federation_benchmarks.rs`
- `query_performance_tests.rs`

### 4.5 `tests/example/` 未接线

- `router_merge_test.rs`

### 4.6 `tests/common/` 辅助文件未直接接线

证据：[common/mod.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/common/mod.rs)

- `assertions.rs`
- `fixtures.rs`
- `mock_db.rs`

---

## 五、直接结论

1. 当前测试“存在量”仍显著大于“执行量”，但首批关键未接线测试已补入主入口。
2. `e2e` 已纳入常规测试入口，`performance` 目录目前仍不参与常规 `cargo test` 执行。
3. `unit` 与 `integration` 目录内仍有多组文件未纳入入口。
4. 后续所有测试完成度结论必须区分：
   - 文件存在
   - 文件已接线
   - 文件已执行
   - 文件执行通过

---

## 六、整改建议

1. 下一批优先处理未接线测试：`performance/*`
2. 将 `performance` 明确拆分为手动性能测试与 Criterion 基准入口，避免与常规测试链路混用
3. 为测试报告增加“接线状态”字段，避免把未执行文件计入完成度
4. 在 CI 中区分主回归套件与可选/手动套件

---

## 七、本轮已补齐并验证的测试接线

已补齐以下测试接线，并通过目标测试验证：

- `tests/unit/auth_service_tests.rs`
- `tests/unit/room_service_tests.rs`
- `tests/unit/search_service_tests.rs`
- `tests/unit/sync_service_tests.rs`
- `tests/unit/invite_blocklist_tests.rs`
- `tests/integration/database_integrity_tests.rs`
- `tests/integration/federation_error_tests.rs`
- `tests/integration/missing_features_tests.rs`
- `tests/e2e/mod.rs`

验证方式：

- `cargo test --test unit auth_service -- --nocapture`
- `cargo test --test unit room_service -- --nocapture`
- `cargo test --test unit search_service -- --nocapture`
- `cargo test --test unit sync_service -- --nocapture`
- `cargo test --test unit invite_blocklist -- --nocapture`
- `cargo test --test integration database_integrity -- --nocapture`
- `cargo test --test integration federation_error -- --nocapture`
- `cargo test --test integration missing_features -- --nocapture`
- `cargo test --test e2e -- --nocapture`
