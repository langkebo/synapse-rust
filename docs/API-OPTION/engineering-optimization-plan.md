# synapse-rust 后端工程化优化方案

> 版本: 1.0  
> 日期: 2026-03-27  
> 工程师: engineering-backend-architect  
> 目标: P0 稳定化 → P1 结构化收敛 → P2 规范化

---

## 一、问题诊断总结

### 1.1 当前状态确认

| 检查项 | 状态 | 说明 |
|--------|------|------|
| `cargo check --all-features` | ✅ 通过 | 编译正常 |
| `cargo fmt --all -- --check` | ✅ 通过 | 格式已修正 |
| `cargo clippy --all-features` | ✅ 通过 | 无警告 |
| `cargo test --all-features` | ✅ 通过 | 全量测试通过 |

**结论**: 静态检查问题已修复，但工程化问题仍需系统性解决。

### 1.2 问题分布图

```
┌─────────────────────────────────────────────────────────────┐
│                      P0: 质量门禁失效                       │
├─────────────────────────────────────────────────────────────┤
│  - CI 阻断点需补强（迁移失败不可忽略、统一 --locked）        │
│  - Cargo.lock 已跟踪，需在 CI 强制 --locked                  │
│  - 敏感文件风险 (已清理)                                   │
│  - 根目录污染 (未复现；由 CI 增加阻断检查)                  │
├─────────────────────────────────────────────────────────────┤
│                      P1: 结构化问题                         │
├─────────────────────────────────────────────────────────────┤
│  - mod.rs 4756 行 / config.rs 4017 行 (上帝文件)           │
│  - services/storage 边界打穿                               │
│  - OIDC 登录未闭环                                          │
│  - Telemetry 指标未初始化                                  │
│  - 240+ 处 unwrap/expect                                   │
├─────────────────────────────────────────────────────────────┤
│                      P2: 规范漂移                           │
├─────────────────────────────────────────────────────────────┤
│  - README 引用不存在的文件                                 │
│  - migrations/README 与实际不符                            │
│  - join_rule/join_rules 命名漂移                          │
│  - 文档无权威索引                                          │
└─────────────────────────────────────────────────────────────┘
```

---

## 二、P0 稳定化方案

### 2.1 CI 质量门禁强化

**问题**: CI 已存在，但需要保证“真阻断 + 可复现构建”

**方案**: 以仓库现有 [.github/workflows/ci.yml](file:///Users/ljf/Desktop/hu/synapse-rust/.github/workflows/ci.yml) 为准，补强并约束：

- 数据库迁移失败不可忽略（`sqlx migrate run` 必须阻断）
- 所有 cargo 构建路径统一强制 `--locked`（避免依赖漂移）
- 增加 Repo Sanity 检查（私钥特征、禁止跟踪 `.key/.pem/.p12`、根目录 `*.db/axum_test.rs` 污染）
- 安全审计仅告警不中断（避免因上游漏洞导致无法合并）
- 同步对齐其他工作流（`test.yml`/`benchmark.yml`）的 toolchain、缓存与 `--locked` 策略，避免“主 CI 通过但旁路工作流失败”

### 2.2 Cargo.lock 纳入版本控制

**现状**: Cargo.lock 已在仓库中，且 `.gitignore` 未忽略该文件

**方案**: 在 CI 强制使用 `--locked`；日常开发只在依赖变更时更新 Cargo.lock，并随代码提交

### 2.3 根目录污染清理

**现状**: 根目录未发现 `axum_test.rs` 或 `*.db`

**方案**: 由 CI Repo Sanity 阻断此类污染；仓库侧 `.gitignore` 统一忽略 `*.db`；如确认 `axum_test` 不再需要，可从 `.gitignore` 移除对应忽略项

### 2.4 P0 验收标准

```
✅ cargo fmt --all -- --check 通过
✅ cargo clippy --all-features -- -D warnings 通过  
✅ cargo test --all-features 通过
✅ Cargo.lock 已纳入版本控制
✅ 根目录无构建产物
✅ CI pipeline 每次 PR 强制执行
✅ CI 阻断任何被跟踪的 `.key/.pem/.p12` 文件
```

---

## 三、P1 结构化收敛方案

### 3.1 上帝文件拆分

#### 3.1.1 web/routes/mod.rs (4756 行)

**问题**: 既做模块汇总，又承载通用提取器、应用状态与路由装配

**方案**: 拆分职责

```
src/web/routes/
├── mod.rs                 # 仅做模块导出 (目标: ~200 行)
├── extractors/            # 通用提取器
│   ├── mod.rs
│   ├── auth.rs           # 认证相关
│   ├── pagination.rs     # 分页参数
│   └── room.rs           # 房间相关
├── state.rs               # 应用状态（第一步先落单文件，后续可再拆目录）
└── assembly.rs            # 路由装配（第一步先落单文件，后续可再拆目录）
```

**实施步骤**:

```rust
// src/web/routes/extractors/auth.rs
pub struct AuthenticatedUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub access_token: String,
}

// src/web/routes/assembly.rs
pub fn create_router(state: AppState) -> Router {
    Router::new().merge(create_auth_router()).with_state(state)
}
```

#### 3.1.2 common/config.rs (4017 行)

**方案**: 按配置域拆分

```
src/common/config/
├── mod.rs                # 导出
├── server.rs             # 服务器配置
├── database.rs           # 数据库配置
├── security.rs           # 安全配置 (TLS, JWT)
├── federation.rs         # 联邦配置
├── cache.rs              # 缓存配置
└── logging.rs            # 日志配置
```

#### 3.1.3 services/mod.rs (1081 行 → 目标: ~300 行)

**方案**: 提取 ServiceContainer 到独立文件

```rust
// src/services/container.rs
pub struct ServiceContainer {
    pub room_service: Arc<RoomService>,
    pub user_service: Arc<UserService>,
    pub message_service: Arc<MessageService>,
    pub presence_service: Arc<PresenceService>,
    pub device_service: Arc<DeviceService>,
    pub media_service: Arc<MediaService>,
    // ... 其他服务
}

impl ServiceContainer {
    pub fn new(config: &Config) -> Self {
        // 构造逻辑
    }
}
```

### 3.2 服务层与存储层边界修复

#### 3.2.1 问题定位

```rust
// room_service.rs:L385-L430 直接执行 SQL
sqlx::query("UPDATE rooms SET name = $1 WHERE room_id = $2")

// services/mod.rs:L800-L989 直接定义 PresenceStorage
pub struct PresenceStorage { ... }
```

#### 3.2.2 修复方案

```rust
// 方案: 服务层不应直接执行 SQL
// 错误示例:
impl RoomService {
    pub async fn update_room_name(&self, name: &str, room_id: &str) -> Result<()> {
        sqlx::query("UPDATE rooms SET name = $1 WHERE room_id = $2")
            .bind(name)
            .bind(room_id)
            .execute(&self.pool)  // ❌ 直接执行
            .await?;
        Ok(())
    }
}

// 正确示例:
impl RoomService {
    pub async fn update_room_name(&self, name: &str, room_id: &str) -> Result<()> {
        self.room_storage.update_name(name, room_id).await  // ✅ 委托存储层
    }
}
```

#### 3.2.3 执行步骤

1. 扫描 services/ 中所有直接 sqlx::query 调用
2. 迁移到对应 Storage 模块
3. 删除 services/mod.rs 中的 Storage 定义
4. 确保所有数据操作经由 Storage 层

### 3.3 未闭环能力处理

#### 3.3.1 OIDC 登录流程

**问题**: oidc.rs:L297-L310 尚未生成设备 ID 和 Matrix Access Token

**方案**: 完成实现或标记明确状态

```rust
// 方案 A: 完成实现
pub async fn complete_login(&self, auth_code: &str) -> Result<LoginResponse> {
    let token = self.exchange_code_for_token(auth_code).await?;
    let userinfo = self.get_userinfo(&token).await?;
    
    // 生成设备 ID
    let device_id = self.generate_device_id();
    
    // 生成 Matrix Access Token
    let access_token = self.generate_access_token(&userinfo.sub, &device_id)?;
    
    // 注册设备
    self.device_service.register_device(&userinfo.sub, &device_id, &token).await?;
    
    Ok(LoginResponse {
        user_id: userinfo.sub,
        device_id,
        access_token,
        // ...
    })
}

// 方案 B: 明确标记为 WIP
#[cfg(feature = "oidc-wip")]
impl OIDCService {
    /// ⚠️ WIP: 设备注册与 Token 生成尚未完成
    pub async fn complete_login(&self, _auth_code: &str) -> Result<LoginResponse> {
        Err(ApiError::not_implemented(
            "OIDC device registration is work-in-progress".to_string()
        ))
    }
}
```

#### 3.3.2 Telemetry 指标初始化

**问题**: telemetry_service.rs:L145 TODO

**方案**: 实现或移除

```rust
// 方案 A: 实现 OTLP
pub fn init_metrics(&self, config: &TelemetryConfig) -> Result<()> {
    #[cfg(feature = "otlp")]
    {
        let exporter = opentelemetry_otlp::new_exporter()
            .with_endpoint(&config.otlp_endpoint)
            .with_timeout(Duration::from_secs(3));
        
        let provider = MeterProvider::builder()
            .with_exporter(exporter)
            .build();
        
        Ok(())
    }
    
    #[cfg(not(feature = "otlp"))]
    {
        // 使用默认 Prometheus 指标
        Ok(())
    }
}

// 方案 B: 移除 TODO，标记为可选
/// Metrics are currently exported via default logging.
/// OTLP export is available with the `otlp` feature flag.
pub fn init_metrics(&self, _config: &TelemetryConfig) -> Result<()> {
    tracing::info!("Using default metrics collection");
    Ok(())
}
```

### 3.4 unwrap/expect 消除计划

#### 3.4.1 统计

```bash
# 统计 unwrap/expect 使用
grep -rn "\.unwrap()\|\.expect(" src/ | wc -l  # 约 240 处
```

#### 3.4.2 分类处理

| 类别 | 数量 | 处理方式 |
|------|------|----------|
| 确定性安全 (如 Arc::clone, 已验证的 Option) | ~60% | 保留，使用 expect 加说明 |
| 潜在风险 (外部输入解析) | ~30% | 改为 if let / ? 错误传播 |
| 严重风险 (内存、网络) | ~10% | 立即修复，使用 anyhow/thiserror |

#### 3.4.3 实施工具

```rust
// 创建错误类型
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Room not found: {0}")]
    RoomNotFound(String),
    
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    
    // ...
}

// 替换示例
// 错误:
let id = params.get("id").unwrap();  // ❌

// 正确:
let id = params.get("id")
    .ok_or_else(|| ApiError::bad_request("Missing 'id' parameter".to_string()))?;  // ✅
```

---

## 四、P2 规范化方案

### 4.1 文档与代码对齐

#### 4.1.1 修复 README 引用

```markdown
# 修复前:
请参阅 docs/synapse-rust/implementation-guide.md

# 修复后:
请参阅 docs/synapse-rust/ARCHITECTURE.md
# 或创建缺失文件
```

#### 4.1.2 创建文档索引

```markdown
# docs/synapse-rust/README.md
# 文档索引

## 架构
- [ARCHITECTURE.md](./ARCHITECTURE.md) - 系统架构
- [DATABASE.md](./DATABASE.md) - 数据库设计

## API
- [API_COVERAGE_REPORT.md](./API_COVERAGE_REPORT.md) - API 覆盖率
- [ADMIN_API_MERGE_PLAN.md](./ADMIN_API_MERGE_PLAN.md) - API 合并计划

## 迁移
- [migrations/README.md](./migrations/README.md) - 迁移说明

## 待完成 (已废弃)
- implementation-guide.md ❌ 已删除
- unfinished_tasks/ ❌ 已删除
```

### 4.2 规范冲突解决

#### 4.2.1 join_rule vs join_rules

**问题**: storage/space.rs 使用 join_rules，测试使用 join_rule

**方案**: 统一为 join_rule (单数)

```rust
// storage/space.rs 修复
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Space {
    // 修复: join_rule (单数)
    pub join_rule: String,  // 而非 join_rules
    // ...
}
```

#### 4.2.2 时间字段命名规范

**问题**: migrations/README 要求 *_ts，规则规定可选时间戳用 _at

**决策**: 统一规则

| 类型 | 格式 | 示例 |
|------|------|------|
| 必需时间戳 | *_ts | created_ts, updated_ts |
| 可选时间戳 | *_at | last_active_at, expires_at |
| 日期时间 | *_dt | login_dt |

```markdown
# 字段命名规范 (更新)
## 时间字段
- 必需: `created_ts`, `updated_ts` (使用 *_ts)
- 可选: `last_active_at`, `expires_at` (使用 *_at)
- 禁止混用，统一在此文档定义
```

### 4.3 仓库清理

#### 4.3.1 目录结构优化

```
synapse-rust/
├── src/                    # 源代码 ✅
├── tests/                  # 集成测试
├── docs/
│   └── synapse-rust/       # 官方文档
│       ├── ARCHITECTURE.md
│       ├── DATABASE.md
│       └── API-OPTION/     # 优化方案
├── migrations/             # 数据库迁移
├── docker/                 # Docker 配置 (无敏感文件)
├── target/                 # 构建产物 (已 gitignore)
├── Cargo.lock              # 依赖锁定 ✅
└── README.md               # 项目入口
```

#### 4.3.2 敏感文件检查清单

```bash
# 本地检查命令（确保这些文件不在 Git 跟踪范围内）
git ls-files '*.key' '*.pem' '*.p12'

# 内容特征检查（防止误提交私钥内容）
grep -R "BEGIN.*PRIVATE KEY" . --include="*.key" --include="*.pem" --include="*.p12"
grep -R "BEGIN RSA PRIVATE KEY" . --include="*.key" --include="*.pem" --include="*.p12"

# 确保以下文件不在仓库:
# docker/ssl/*.key (私钥)
# docker/data/keys/*.key (签名密钥)
# 任何包含 BEGIN * PRIVATE KEY 的文件
```

---

## 五、实施路线图

### 5.1 阶段一: P0 稳定化 (第 1-2 周)

| 任务 | 负责人 | 验收 |
|------|--------|------|
| 修复 CI 配置，确保 fmt/clippy/test 阻断 | 龙卷风 | PR 必须通过 CI |
| 将 Cargo.lock 纳入版本控制 | 龙卷风 | git status 显示 Cargo.lock |
| 清理根目录污染 (axum_test.rs) | 龙卷风 | ls *.rs 根目录无测试文件 |
| 添加敏感文件检查到 CI | 龙卷风 | CI 检查密钥文件 |

### 5.2 阶段二: P1 结构化收敛 (第 3-6 周)

| 任务 | 负责人 | 验收 |
|------|--------|------|
| 拆分 web/routes/mod.rs | 龙卷风 | routes/extractors/, routes/assembly.rs, routes/state.rs |
| 拆分 common/config.rs | 龙卷风 | config/server.rs, config/database.rs 等 |
| 提取 services/container.rs | 龙卷风 | ServiceContainer 独立文件 |
| 修复 services 直接执行 SQL | 龙卷风 | 0 处直接 sqlx::query |
| 完成 OIDC 登录或标记 WIP | 龙卷风 | oidc_service 无 TODO panic |
| 完成 Telemetry 或移除 TODO | 龙卷风 | telemetry_service 无 TODO |
| 消除高风险 unwrap (30%) | 龙卷风 | 风险 unwrap < 80 处 |

### 5.3 阶段三: P2 规范化 (第 7-8 周)

| 任务 | 负责人 | 验收 |
|------|--------|------|
| 修复 README 引用 | 龙卷风 | 所有链接有效 |
| 创建文档索引 | 龙卷风 | docs/README.md 存在 |
| 统一 join_rule 命名 | 龙卷风 | 测试编译通过 |
| 统一时间字段规范 | 龙卷风 | 文档与代码一致 |
| 仓库清理 | 龙卷风 | 无冗余报告/脚本 |

---

## 六、验收检查清单

### 6.1 P0 验收

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-features -- -D warnings` 通过
- [ ] `cargo test --all-features` 通过
- [ ] Cargo.lock 在仓库中
- [ ] CI pipeline 对每个 PR 执行完整检查

### 6.2 P1 验收

- [ ] routes/mod.rs < 2000 行
- [ ] config.rs < 2000 行
- [ ] services/mod.rs < 500 行
- [ ] services/ 无直接 sqlx::query
- [ ] OIDC 登录闭环 (可用或明确 WIP)
- [ ] Telemetry 指标初始化完成
- [ ] unwrap/expect 使用 < 160 处

### 6.3 P2 验收

- [ ] README 所有链接有效
- [ ] 文档索引存在
- [ ] join_rule 命名统一
- [ ] 无敏感文件驻留仓库
- [ ] 根目录无构建产物

---

## 七、风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|----------|
| 拆分破坏现有功能 | 高 | 全面回归测试 |
| 服务层重构影响业务 | 中 | 逐步迁移，保持兼容 |
| 文档更新引入新错误 | 低 | 审查流程 |
| CI 强化导致历史 PR 无法合并 | 中 | 批量修复或跳过 |

---

## 九、已完成工作总结 (2026-03-28)

### 9.1 本次会话完成的工作

#### P1: join_rule 命名规范统一 ✅

**问题**: `Room`、`RoomSummary` 等结构体使用 `join_rules` (复数)，与 Matrix 规范不符

**修复内容**:
| 文件 | 修改内容 |
|------|----------|
| `src/storage/room.rs` | `Room` 结构体字段 `join_rules` → `join_rule` |
| `src/storage/room_summary.rs` | `RoomSummary`、`CreateRoomSummaryRequest`、`UpdateRoomSummaryRequest` 等字段统一 |
| `src/auth/authorization.rs` | 字段访问更新 |
| `src/services/room_service.rs` | JSON 响应映射更新 |
| `src/services/sync_service.rs` | JSON 响应映射更新 |
| `src/services/room_summary_service.rs` | 请求/响应结构更新 |
| `src/web/routes/admin/room.rs` | JSON 响应映射更新 |
| `src/web/routes/federation.rs` | 字段访问更新 |
| `src/storage/mod.rs` | 测试代码更新 |

**验证结果**: `cargo test --all-features` 753 tests passed

---

#### P1: routes/mod.rs Handler 提取 ✅ (部分)

**成功提取的模块**:

| 模块 | 文件 | 减少行数 |
|------|------|----------|
| Push Rules | `push_rules.rs` | ~125行 |
| Validators | `validators.rs` | ~95行 |

**未成功提取**:
- Sync Handlers: Axum Handler trait 兼容性问题，需要同时修改 `assembly.rs` 的路由注册方式

---

#### P1: middleware.rs 拆分方案分析 ✅

**结论**: middleware.rs 存在大量交叉引用函数，直接拆分会引入循环依赖

**建议方案**: 分阶段提取
1. Phase 1: 提取 Utils 工具函数 (无依赖风险)
2. Phase 2: 提取 CORS 模块
3. Phase 3: 提取 Federation 签名验证 (风险高)
4. Phase 4: 提取 Security 中间件

---

### 9.2 当前文件状态

| 文件 | 当前行数 | 原行数 | 变化 |
|------|----------|--------|------|
| `src/web/routes/mod.rs` | 3873 | 4756 | -883 行 |
| `src/web/routes/push_rules.rs` | 156 | - | 新增 |
| `src/web/routes/validators.rs` | 152 | - | 新增 |
| `src/web/middleware.rs` | 2161 | 2161 | 未变 |

---

## 十、下一步工作计划

### 优先级排序

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | routes/mod.rs 继续拆分 | 待处理 | 可尝试提取 Auth/Profile handlers |
| P0 | middleware.rs Phase 1 (Utils) | 待处理 | 需重试，验证方案可行性 |
| P1 | Sync Handlers 提取 | 待处理 | 需同时修改 assembly.rs |
| P1 | config.rs 拆分 | 待处理 | 按配置域拆分 |
| P2 | services/mod.rs 拆分 | 待处理 | ServiceContainer 已独立 |
| P2 | OIDC 登录完善 | 待处理 | 标记 WIP 或完成实现 |
| P2 | Telemetry 指标完善 | 待处理 | 标记可选或实现 OTLP |

---

### routes/mod.rs 剩余 Handler 组

| Handler组 | 起始行 | 估计行数 | 说明 |
|-----------|--------|----------|------|
| Auth | 453 | ~200 | register, login, logout, whoami |
| Profile/Account | 880 | ~400 | profile, password, threepid |
| Directory/Reporting | 1350 | ~200 | search, report |
| Sync | 1600 | ~120 | sync, filter, events |
| Room | 1720 | ~1100 | 35个房间相关handlers |
| Presence | 2720 | ~300 | presence, state |
| Moderation | 3510 | ~200 | kick, ban, unban |

### 现有 handlers/ 目录结构

项目已有 `src/web/routes/handlers/` 目录，包含简化版处理器：

```
src/web/routes/handlers/
├── mod.rs        # 模块导出
├── auth.rs       # 简化认证处理器
├── health.rs     # 健康检查
├── room.rs       # 简化房间处理器
├── user.rs       # 简化用户处理器
└── versions.rs  # 版本信息
```

**注意**: handlers/ 中的处理器为简化存根，routes/mod.rs 中的是完整实现。拆分时需注意区分。

---

### 8.1 关键文件位置

- `docs/API-OPTION/README.md` - API 优化方案汇总
- `docs/synapse-rust/database-design.md` - 数据库设计
- `migrations/README.md` - 迁移说明

---

**下一步**: 确认方案后，开始阶段一 P0 稳定化工作
