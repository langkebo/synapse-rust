# Matrix 服务审计与依赖收敛建议

> 日期: 2026-04-16
> 范围: 非 `openclaw` 专项
> 目标: 先保证当前代码无错误，再梳理 Matrix 服务边界与依赖收敛优先级

## 本轮结论

- `cargo check --all-targets --all-features` 通过
- `cargo clippy --all-targets --all-features -- -D warnings` 通过
- `cargo test --test integration api_federation_` 通过，结果为 `60 passed; 0 failed`
- `cargo machete` 复扫通过，当前无未使用直接依赖
- 未发现新的 `/_matrix/client/...` 可达 admin-only 行为
- 未发现 federation `get_state` / `get_state_ids` / `backfill` 的最小披露回归
- 未发现 `keys/query`、`keys/claim`、federation `user/keys/query` 的最小披露回归
- `cas.rs` 中遗留裸 `/admin/*` 管理别名继续保留兼容，但已显式标记为弃用

## Matrix 服务边界审计

### 1. client/admin 可达性

本轮重点复核了 `directory_reporting` 与相邻目录路由：

- 举报事件、举报房间、扫描信息、房间别名写操作均要求 `ensure_room_member`
- 未发现 `AdminUser` 被直接挂载到 `/_matrix/client/...`
- 目录相关 handler 仍通过 client 路由装配，不存在误挂到 admin 路由树的情况

建议:

- 保持现有边界，不进行额外放宽
- 后续若新增 client 路由，优先复用成员态检查 helper，而不是在 handler 内手写条件分支

### 2. federation 最小披露

已复核以下服务侧接口：

- `get_state`
- `get_state_ids`
- `backfill`
- `keys/query`
- 兼容/遗留 `keys/query`、`keys/claim` 拒绝路径

结论:

- `get_state` / `get_state_ids` / `backfill` 都建立在 `validate_federation_origin_in_room(...)` 之上
- `backfill` 只从给定事件之前回溯，不存在无界历史外泄
- federation `keys/query` 会先把请求用户集裁剪为“本地用户且与请求源共享房间”
- client `keys/query` / `keys/claim` 也先基于共享房间过滤目标用户

建议:

- 后续若新增 federation 读取接口，默认先复用 `validate_federation_origin_in_room(...)`
- 对“用户级资料读取”保持“本地用户 + shared room”双条件，不要退回仅校验本地域名

### 3. CAS 管理路径兼容性

现状:

- `cas.rs` 同时提供标准管理前缀 `/_synapse/admin/v1/cas/*`
- 还保留了一组历史兼容别名 `/admin/services`、`/admin/users/{user_id}/attributes`
- 两组路径都经过同一层 `admin_auth_middleware`，因此当前不存在“裸路径可被普通用户访问”的越权问题

本轮处理:

- 保留 legacy `/admin/*` 别名，避免直接破坏旧调用方
- 为 legacy 别名统一添加 `Deprecation: true`
- 同时添加 `Warning: 299 ... use /_synapse/admin/v1/cas/*`，向调用方明确迁移目标

判断:

- 这项更像“命名空间治理和未来误接线风险”而不是即时权限漏洞
- 用响应头显式弃用比直接删除更稳妥，也更适合作为标准前缀迁移的过渡阶段

建议:

1. 新接入方只使用 `/_synapse/admin/v1/cas/*`
2. 继续保留 `/admin/*` 一个迁移窗口，但不再为其扩展新能力
3. 后续若确认无外部依赖，再评估移除 legacy 别名并保留回归测试锁定行为

## 依赖收敛分析

本轮基于 `cargo tree -d` 与反向依赖查询做了分层判断。

### A. 已处理: `deadpool` 直接依赖残留

现状:

- 项目源码中未发现 `deadpool-postgres` / `tokio-postgres` 的实际调用点
- 删除 `Cargo.toml` 中的 `deadpool-postgres` 后，`cargo check --all-targets --all-features` 仍通过
- `cargo clippy --all-targets --all-features -- -D warnings` 仍通过
- `cargo test --test integration api_federation_` 仍通过
- 当前保留的 `deadpool v0.12.3` 来自 `deadpool-redis v0.18.0` 与 `wiremock` (dev)

判断:

- 这次问题不是“需要升级”，而是“未使用依赖残留”
- 直接删除比升级更低风险，也已经带来一组重复依赖收敛

建议动作:

1. 保持 `deadpool-postgres` 移除状态，不重新引入
2. 后续若确实需要 PostgreSQL 连接池库，再基于真实调用点决定是否引入
3. 后续依赖收敛优先看仍然存在的上游链路分叉

### B. 已处理: 其余未使用直接依赖

现状:

- 基于 `cargo machete` 与源码反查，已删除 10 项未使用直接依赖:
  `elasticsearch`、`futures-util`、`metrics-exporter-prometheus`、
  `opentelemetry-semantic-conventions`、`rsa`、`serde_with`、`subtle`、
  `tokio-util`、`tower_governor`、`tracing-appender`
- `server` feature 已同步移除 `tower_governor` 绑定
- 复扫后 `cargo machete` 未再报告未使用直接依赖
- `thiserror v1` 已随相关链路移除而自然消失，当前仅剩 `thiserror v2.0.18`

判断:

- 这些项属于“源码无调用、配置残留”的低风险清理
- 直接删除比保留观察更稳妥，也顺带减少了历史依赖分叉

建议:

- 保持当前收敛状态
- 后续将 `cargo machete` 纳入常规依赖健康检查

### C. 已不再是当前问题: `whoami` 双版本

现状:

- 清理 `deadpool-postgres` 后，`whoami v2.1.1` 链路已随之消失
- 当前仅保留 `sqlx-postgres` 带来的 `whoami v1.6.1`

判断:

- 该项已经随未使用依赖清理自然收敛
- 无需再单独治理

建议:

- 保持现状

## 架构建议

### 1. 保持双层边界模式

对于 Matrix 服务端接口，继续维持两层判断：

- 路由/提取器层做“谁能进来”的基础认证
- handler/service 层做“能看到哪些数据”的最小披露裁剪

这样可以避免：

### 2. 明确区分 federation 占位接口的处理策略

当前 `federation.rs` 中仍有少量“已挂载但明确拒绝”的接口，分为两类：

- 建议保留显式拒绝:
  - `/_matrix/federation/v1/keys/upload`
  - `/_matrix/federation/v1/keys/claim`
  - `/_matrix/federation/v1/keys/query`
- 原因:
  - 这些路径对应历史/兼容调用面，当前实现返回 `ApiError::unrecognized(...)`
  - 比起静默 404，更有利于调用方快速迁移到已支持的 `user/keys/*` 路径
  - 现有返回不暴露额外数据，仅提供迁移指引，风险较低

- 可继续观察是否直接撤路由:
  - `/_matrix/federation/v1/query/destination`
  - `/_matrix/federation/v1/query/auth`
  - `/_matrix/federation/v1/event_auth`
- 原因:
  - 这几项当前返回 `ApiError::not_found(...)`
  - 从行为上看，与“不挂载路由”差异有限，保留主要价值在于日志与诊断信息更明确
  - 若后续想进一步收紧暴露面，可评估直接移除路由并补一条回归测试确认 404 行为不变

- 认证正确但数据暴露过宽
- 路由挂载正确但 handler 忽略成员态/共享房间条件

### 2. 逐步收敛数据库访问栈

当前数据库相关能力主要依赖：

- `sqlx`
- `deadpool-redis`

建议中期目标:

- 保持 PostgreSQL 访问以 `sqlx` 为主，不重新引入未落地的双轨方案
- 若未来需要新增连接池方案，先证明真实场景收益，再决定是否引入
- 对剩余重复依赖接受“上游导致”的现实，不为零重复做高风险改造

### 3. 将 `openclaw` 继续留作最后专项

本轮没有把 `openclaw` 纳入修改范围，原因如下：

- 当前非 `openclaw` 主干已达到“可编译、可回归、边界无新回归”的状态
- `openclaw` 涉及 feature gate、运行时 gate、非稳定命名空间和测试夹具，适合单独收口
- 混在本轮依赖/边界整治中处理，会降低回归定位效率

## 后续执行顺序

推荐下一步按以下顺序推进：

1. 保持当前依赖收敛结果并观察是否还有新的未使用直接依赖
2. 若继续做收敛，优先针对剩余上游重复链路输出建议而非强改
3. 保持 Matrix 服务边界不再扩散后，再进入 `openclaw` 最终专项
