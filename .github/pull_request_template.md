## Pull Request Checklist

### 代码质量
- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-features --locked -- -D warnings` 通过
- [ ] `cargo check --workspace --all-features --locked` 通过
- [ ] `cargo test --features test-utils --test unit` 通过
- [ ] 新增代码有对应的测试覆盖

### 文档状态同步
- [ ] 若涉及公共 API 变更，已更新 `docs/synapse-rust/API_COVERAGE_REPORT.md`
- [ ] 若涉及数据库 schema 变更，已更新迁移文件
- [ ] 若涉及路由变更，已更新 `route_ledger` 与路由清单
- [ ] 若涉及技术债修复，已更新 `docs/audit/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md` 中对应任务状态
- [ ] 若涉及分层迁移，已运行 `python3 scripts/ci/check_root_canonical_ledger.py` 确认无新增双轨

### 依赖管理
- [ ] 若新增依赖，已确认无重复版本引入（`cargo tree -d --workspace`）
- [ ] 若更新依赖，`cargo update --dry-run` 无意外变更

### 部署与运维
- [ ] 若涉及 worker topology 变更，已更新 `docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md`
- [ ] 若涉及配置项变更，已更新 `homeserver.yaml` 示例
- [ ] 若涉及新 MSC 实现，已在 `docs/audit/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md` 中记录
