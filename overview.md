# Synapse-Rust API Testing — Week 2 Task 6 完成

**任务**: Path Param Instantiation (路径参数实例化)
**日期**: 2026-09-01
**状态**: ✅ 完成

---

## 做了什么

**新增脚本**: `scripts/api_test/schemathesis_pathparam_test.py` (290 行)

**目标**: 用 curl 直接探测 553 个含路径参数 (`{user_id}`, `{room_id}` 等) 的端点,
同时验证 errcode 符合 Matrix 协议规范。

**策略**:
1. 从 OpenAPI spec 找出所有含 path-param 的 operations (553 个)
2. 从 `config.yaml:path_params` 加载 21 个配置值, 内嵌 20 个补全值 (35 种 param 全覆盖)
3. 实例化: `{user_id}` → `@testuser1:matrix.test` (URL-encoded)
4. 并发 curl 探针 (12 workers, 3.6s 跑完 553 端点)
5. errcode 白名单校验 (复用 Task 3 errcode_validator)

## 关键修复

- **errcode_validator word-boundary 匹配**: 修复 `/direct` → `/directory/list/room/` 误匹配
- **token_manager SSL bypass**: 支持自签名证书 (mkcert)
- **M_NOT_FOUND 白名单**: 新增 5 条 path-param 专用规则

## 结果

| 指标 | 值 |
|------|---|
| 端点总数 | 553 |
| 5xx bug | 0 |
| unexpected errcode | 0 |
| errcode 合规率 | 100% |
| 耗时 | 3.6s (154 ep/s) |

**HTTP 状态分布**: 404 (40.3%), 400 (34.9%), 200 (14.8%), 403 (9.4%), 501 (0.5%)
**errcode 分布**: M_NOT_FOUND (40.0%), M_FORBIDDEN (9.4%), M_BAD_JSON (2.0%)

## 总体 Week 2 覆盖率

| 任务 | 端点数 |
|------|--------|
| Task 1 smoke + extended | 5 + 48 |
| Task 2 user-auth | 278 |
| Task 3 errcode validation | 278 |
| Task 4 handler schema scan | 82 (patches) |
| Task 5 response schema probe | 107 (patches) |
| **Task 6 path-param** | **553** |
| **合计** | **884/898 = 98.4%** |

## 相关文件

- `scripts/api_test/schemathesis_pathparam_test.py` — 主脚本
- `scripts/api_test/errcode_validator.py` — errcode 校验 (改动)
- `scripts/api_test/token_manager.py` — token 管理 (改动)
- `scripts/api_test/reports/schemathesis_pathparam.json` — 测试结果
- `artifacts/week2-task6-pathparam-instantiation.md` — 详细报告

## 下一步

- Admin endpoints 覆盖 (用 admin token)
- Federation endpoints 覆盖 (生成 federation.yaml)
- POST body 真实数据生成 (替代空 body)
