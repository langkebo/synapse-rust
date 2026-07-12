# 阶段 0 基线摘要 (2026-07-10)

## Git 基线
- 分支: `feat/architecture-optimization-round2`
- HEAD: `e63ebf3e` `refactor(storage): split media_quota.rs god-file into submodules`
- 工作树: 172 个文件变更 (120 删除 + 48 修改 + 4 未跟踪)
- 完整快照: [00_git_baseline.txt](00_git_baseline.txt)

## Docker 服务状态
- docker-postgres: healthy (8 days), 端口 15432
- docker-redis: healthy (9 days), 端口未映射到宿主机（应用使用 in-memory fallback）
- docker-rust: healthy (synapse-rust:latest)
- docker-nginx: healthy (80/443/8448)
- docker-element-web: healthy (8080)

## 测试基线
| 测试目标 | 结果 | 通过 | 失败 | 耗时 |
|---------|------|------|------|------|
| lib (库单元测试) | ✅ PASS | 465 | 0 | 10.39s |
| unit (tests/unit/) | ✅ PASS | 867 | 0 | 0.46s |
| integration (tests/integration/) | ❌ FAIL | - | 158 | 超时停止 (~2h) |

### Integration 测试问题分析
- **158 个 FAILED**
- **122 个测试卡住 >60s**（DB 连接池争用）
- 失败集中在 DB 相关测试：storage_tests_migrated、feature_flag、federation_blacklist、filter_storage、beacon_storage、captcha、auth_service、db_schema_smoke、event_storage、device_storage
- API 层失败：api_media_routes_tests（全部失败）、api_federation_tests、api_appservice_tests、api_device_routes_tests、api_device_presence_tests

### 根因假设（待阶段 9 系统化调试验证）
1. DB 连接池耗尽（测试并行 + max=32 连接）
2. 测试数据污染（前一轮测试遗留数据，未用 TestContext 隔离）
3. 部分可能是前一轮架构优化进行中工作引入的回归

完整日志: [00_test_baseline.log](00_test_baseline.log)

## Clippy 基线
- 结果: ❌ 1 个错误
- 错误: `synapse-services/src/sync_service/api_trait.rs:7` too_many_arguments (8/7)
- 来源: 未跟踪的新文件（前一轮 round2 进行中工作）
完整日志: [00_clippy_baseline.log](00_clippy_baseline.log)

## 性能基线
- 状态: ⏳ 待采集（需先确认 benchmark 可编译）

## 结论
当前基线反映"前一轮架构优化进行中"的状态，集成测试有大量失败（主要是 DB 相关）。
这些失败本身就是阶段 3 代码审计和阶段 9 系统化调试的重要输入。
