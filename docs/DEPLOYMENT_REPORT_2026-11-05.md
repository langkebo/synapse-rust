# 全功能零缓存重建部署与验证报告（2026-11-05）

## 1. 基本信息

| 项 | 值 |
| --- | --- |
| 部署方式 | `docker/deploy/deploy.sh --all`（全部功能：core + all extensions） |
| 构建策略 | `--no-cache`、`--target tools` |
| 提交 | `66fcaae32`（`fix(cors): 普通响应与预检共用 Expose-Headers 常量，补齐限流头`） |
| 镜像 | `synapse-rust:6.3.0-66fcaae32`（`org.opencontainers.image.revision=66fcaae32`，`version=6.3.0`） |
| 回滚镜像 | `synapse-rust:rollback-20261105_114358` |
| 日志 | `docker/deploy/logs/deploy_20261105_114358.log` |

## 2. 缓存与镜像清理（部署前）

| 操作 | 结果 |
| --- | --- |
| `rm -rf target` | 本地编译缓存约 73 GB 清除 |
| `docker builder prune -af` | 回收 4.937 GB，Build Cache 归零 |
| `docker container prune -f` / `network prune -f` / `volume prune -f` | 回收 189.5 kB / 0 B / 0 B |
| `docker rm -f synapse-app` + `docker rmi`（旧版本 tag） | 旧后端镜像与 2 个镜像层删除 |
| `docker image prune -af` | 回收 99.74 MB，悬空镜像清零 |

## 3. 部署步骤结果

| 步骤 | 结果 |
| --- | --- |
| 环境依赖 / 配置 / 目录 / SSL / megolm 密钥 / hosts / TURN | 通过 |
| 构建镜像（零缓存） | 482 s |
| 启动服务与迁移 | 62 s |
| 数据库连接验证 / 版本一致性校验 | 通过（canonical=1，已应用 2，失败 0） |
| 健康检查 / HTTPS 接口验证 | 通过 |
| 容器日志 ERROR/WARNING 分析 | 无 ERROR、无未知 WARNING |
| 监控栈 | 正常启动 |

容器（14 个）全部 `healthy`：`synapse-app`、`synapse-postgres`、`synapse-redis`、`synapse-nginx`、`synapse-nginx-proxy`、`synapse-prometheus`、`synapse-grafana`、`synapse-alertmanager`、`synapse-alert-handler`、`synapse-pushgateway`、`synapse-node-exporter`、`synapse-postgres-exporter`、`coturn`、`synapse-test-db`。

## 4. BE 问题清单修复验证（真实接口）

| 编号 | 验证请求 | 实测结果 | 结论 |
| --- | --- | --- | --- |
| BE-1 | `GET /_matrix/vendor/v1/friends?offset=10` | `400 M_INVALID_PARAM`（"Legacy offset pagination is no longer supported; use from cursor"） | 通过 |
| BE-2 | 无 `Authorization` 访问 vendor 接口 | `401 M_MISSING_TOKEN` | 通过 |
| BE-3 | 带 `Idempotency-Key: idem-xyz` 连发两次（含换目标） | 第二次直接返回首次 `request_id` + `idempotent_replay: true`，不新建请求 | 通过 |
| BE-4 | `OPTIONS` 预检请求 `idempotency-key` | `access-control-allow-headers: idempotency-key, content-type` | 通过 |
| BE-5 | `POST /_matrix/client/v3/upload/token` | 200，含 `upload_method: POST`、`download_url_prefix`、`upload_url`、`max_file_size`、回显 `scene`；未声明字段 → `400 M_BAD_JSON` | 通过 |
| BE-8 | 预检与普通响应 `Access-Control-Expose-Headers` | 两处同为 `X-Request-ID, X-CSRF-Token, X-Matrix-Error, Retry-After, X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Retry-After-Ms` | 通过 |
| BE-9 | `ApiError::gone` 响应体 | 统一 `{errcode, error}` 外壳（单测覆盖） | 通过 |
| BE-10 | `GET /_matrix/vendor/v1/friends/dm/@testuser2:matrix.test` | `{"exists":false,"room_id":null,"user_id":"@testuser2:matrix.test"}` | 通过 |

基础连通性：`/health` = 200，`/_matrix/client/versions` = 200，密码登录成功。

## 5. 全功能 API 集成测试

`ADMIN_PASS=*** ./docker/deploy/api-integration_test.sh --profile full`

| 指标 | 值 |
| --- | --- |
| 通过 | 534 |
| 失败 | 0 |
| 跳过 | 66 |
| 缺失（端点不可用） | 2 |
| 硬失败 | 无（脚本结论：`✓ No hard failures detected`） |

跳过原因主要为环境未启用的可选能力：SSO/OIDC/CAS 未配置、Identity Server 未本地托管、builtin OIDC 未启用、破坏性用例、需联邦签名/对端数据等。

## 6. 复核后闭环状态（2026-11-05 后续修复）

| 编号 | 现状 |
| --- | --- |
| BE-2（服务层） | 已闭环：`auth/token.rs`、`refresh_token_service.rs`、`auth/session.rs` 的客户端 token 失败统一 401 `M_UNKNOWN_TOKEN`；已注销用户统一 403 `M_USER_DEACTIVATED`；`M_UNAUTHORIZED` 仅保留 federation 重放检测（已在 `ROUTE_CONTRACT.md` 登记为有意例外） |
| BE-3 | 已闭环：`Idempotency-Key` 参与 `(user_id, key)` 去重（24h TTL），同 key 重放返回首次 `request_id` 并带 `idempotent_replay: true`；`X-Request-ID` 仅用于链路追踪 |
| BE-5（`scene`） | 已闭环：`scene` 原样回显（空串/空白归一为 `null`）；不做分场景限额（仓库无任何 scene 相关配置） |
| BE-6 | 已闭环：响应已移除 `upload_token`，全仓无消费点 |
| BE-7 | 已闭环：`ROUTE_CONTRACT.md` §F.2 已登记 `M_UNKNOWN` 在 400/409/500/501/503 的多义，并有 `M_UNKNOWN_MEANINGS` 穷举守卫测试 |
| C1 / C2 | 已闭环：好友请求列表补 `request_id` / `request_id_num` / `requester` / `recipient` / `created_ts`；`request_id` 字符串与 `request_id_num` 数字双发，SDK 文档同步 |
| BE-11 | 已闭环：`/versions.unstable_features` 声明 `io.hula.friends` / `io.hula.burn_after_read`，取值受 `experimental.declare_private_extensions` 与实际路由面控制（与 `/capabilities` 一致） |
| 测试回归 | 已闭环：`api_friend_routes` / `api_push_pushers` / `api_space_lifecycle` 的过期 401 断言与 `sync` / `join` 两个快照统一为 `M_MISSING_TOKEN`；`versions` 快照补齐 `io.hula.*` 键；新增 BE-3 幂等去重集成测试（同 key 重放返回同一 `request_id` + `idempotent_replay: true`） |

## 7. 结论

零缓存全功能重建部署成功，14 个容器全部健康，镜像 revision 与 `HEAD` 一致；BE-1~BE-11 全部闭环，全功能集成测试 534 通过 / 0 失败。
