# Synapse Rust 后端系统重构与优化方案

**版本**: 1.0
**日期**: 2026-02-08
**状态**: 待执行
**文档位置**: `/Users/ljf/Desktop/hulah/synapse/docs/synapse-rust/BACKEND_REFACTORING_PLAN.md`

---

## 1. 问题分析与分类 (Problem Analysis & Classification)

基于 `api-error.md` 的测试反馈与代码审查，当前系统主要存在以下几类核心问题。我们按优先级（P0-最高）进行分类。

| ID | 问题类别 | 问题描述 | 优先级 | 影响范围 | 涉及模块 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **SEC-01** | **安全隐患** | **任意文件上传风险**：`upload_voice_message` 仅依赖 `Content-Type` 头，未验证文件实际内容（Magic Number），存在恶意脚本上传风险。 | **P0** | 服务器安全 | `web/routes/voice.rs` |
| **SEC-02** | **安全隐患** | **输入验证缺失**：多个 API（好友分类、密钥备份）缺乏对字段长度、特殊字符、空值的校验，存在 XSS 和数据完整性风险。 | **P0** | 数据安全 | `web/routes/friend.rs`, `key_backup.rs` |
| **SEC-03** | **安全隐患** | **错误信息泄露**：数据库约束冲突直接返回 500 及详细 SQL 错误信息，暴露系统内部实现。 | **P1** | 信息安全 | 全局错误处理 |
| **FUN-01** | **功能缺陷** | **业务逻辑错误**：管理员封禁接口因数据库字段 `blocked_at` 缺失导致 500 错误；邮箱验证 URL 生成使用了错误的 Host。 | **P0** | 核心业务 | `web/routes/admin.rs`, `mod.rs` |
| **FUN-02** | **功能缺失** | **音频处理未实现**：音频转码功能仅为 TODO 状态，未实际对接 `ffmpeg` 或相关库。 | **P1** | 核心业务 | `services/voice_service.rs` |
| **ARC-01** | **架构问题** | **数据库管理原始**：依赖硬编码的 SQL 字符串初始化数据库，缺乏版本控制和迁移机制。 | **P1** | 可维护性 | `storage/mod.rs` |
| **ARC-02** | **架构问题** | **配置管理分散**：部分配置（如上传限制）散落在代码中，未统一管理。 | **P2** | 可维护性 | 全局 |

---

## 2. API 接口重构规范 (API Interface Refactoring Standards)

### 2.1 统一请求/响应格式
所有 API 必须遵循统一的 JSON 结构，确保前端解析的一致性。

```rust
// 响应包装结构
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub status: String,      // "ok" 或 "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,     // 成功时的数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>, // 错误时的简要描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<String>, // 标准错误码 (如 M_INVALID_PARAM)
}
```

### 2.2 错误码体系
建立映射表，将内部错误转换为标准 Matrix 错误码。

*   `M_FORBIDDEN` (403): 权限不足
*   `M_UNKNOWN_TOKEN` (401): 认证失败
*   `M_INVALID_PARAM` (400): 输入验证失败
*   `M_LIMIT_EXCEEDED` (429): 请求限流
*   `M_NOT_FOUND` (404): 资源不存在

### 2.3 认证与授权
*   **Trait 设计**: 定义 `Extractor` trait 用于从请求头解析 `Authorization: Bearer <token>`。
*   **JWT 管理**: 使用 `jsonwebtoken` crate，强制验证 `exp` (过期时间) 和 `iss` (签发者)。
*   **Scope 控制**: 在 JWT 中包含 `scope` 字段（如 `admin`, `user`），中间件层自动拦截越权访问。

---

## 3. 安全加固方案 (Security Hardening Scheme)

### 3.1 输入验证 (Input Validation)
引入 `validator` crate，在 DTO (Data Transfer Object) 层强制定义校验规则。

```rust
#[derive(Deserialize, Validate)]
pub struct CreateCategoryReq {
    #[validate(length(min = 1, max = 50), custom = "validate_no_xss")]
    pub name: String,
    #[validate(range(min = 0, max = 100))]
    pub priority: i32,
}
```

### 3.2 SQL 注入防护
*   **严禁**使用 `format!` 拼接 SQL 语句。
*   **强制**使用 `sqlx` 的参数绑定功能 (`.bind()`)。
*   对动态查询条件使用 `QueryBuilder` 构建器。

### 3.3 文件上传安全
*   **Magic Number 检测**: 使用 `infer` crate 读取文件头 262 字节，验证真实 MIME 类型。
*   **文件名清洗**: 移除文件名中的路径分隔符和特殊字符，生成随机 UUID 作为存储文件名。
*   **大小限制**: 在 Nginx 层和 Axum 层双重限制 `Content-Length` (最大 10MB)。

### 3.4 敏感数据保护
*   使用 `secrecy` crate 包装密码、Token 等敏感字段，防止日志意外打印。
*   数据库连接串等机密配置仅通过环境变量注入，禁止硬编码。

---

## 4. 数据库优化策略 (Database Optimization Strategy)

### 4.1 迁移管理 (Migrations)
*   引入 `sqlx-cli`。
*   将 `src/storage/mod.rs` 中的建表语句转化为 `.sql` 迁移文件，存放于 `/migrations` 目录。
*   CI/CD 流程中增加 `sqlx migrate run` 步骤。

### 4.2 索引优化
针对高频查询字段添加索引：
*   `users(username)`: 唯一索引
*   `access_tokens(token)`: 哈希索引
*   `room_memberships(user_id, room_id)`: 复合索引
*   `events(room_id, origin_server_ts)`: 复合索引（用于时间线分页）

### 4.3 连接池调优
配置 `PgPoolOptions`：
*   `max_connections`: 根据 CPU 核数设置为 `CPU * 2 + 1` (微服务场景下需考虑总连接数)。
*   `min_connections`: 保持少量热连接。
*   `acquire_timeout`: 设置为 3秒，避免请求长时间阻塞在获取连接上。

---

## 5. 架构重构设计 (Architecture Refactoring Design)

### 5.1 异步处理模型
充分利用 Rust 的 `Future` 特性：
*   **IO 密集型任务**（DB查询、HTTP请求）：使用 `tokio::spawn` 异步执行。
*   **计算密集型任务**（图像处理、加解密）：使用 `tokio::task::spawn_blocking` 避免阻塞事件循环。

### 5.2 缓存层设计 (Caching Layer)
引入多级缓存策略：
*   **L1 本地缓存**: 使用 `moka` crate，存储极高频且不常变更的数据（如公钥、系统配置）。
*   **L2 分布式缓存**: 引入 `Redis`，存储 Session、频繁访问的用户资料。
*   **缓存一致性**: 采用 "Cache-Aside" 模式，更新数据库后失效缓存。

### 5.3 模块化与解耦
*   **依赖注入**: 继续完善 `ServiceContainer`，但引入 `Trait` 定义服务接口，方便单元测试 Mock。
*   **领域驱动**: 将 `src/services` 按业务领域（User, Room, Chat）进一步拆分，每个领域包含独立的 Model、Service 和 Storage。

---

## 6. 性能优化指标 (Performance Optimization Metrics)

### 6.1 目标指标 (KPIs)
*   **API 响应时间 (P99)**: < 100ms
*   **并发处理能力**: 单实例支持 1000 QPS (Queries Per Second)
*   **内存占用**: 空闲 < 50MB，高负载 < 500MB
*   **冷启动时间**: < 1s

### 6.2 监控方案
*   **Metrics**: 集成 `metrics` crate，暴露 `/metrics` 端点供 Prometheus 抓取。
*   **Tracing**: 使用 `tracing` 和 `tracing-subscriber`，支持 `OpenTelemetry` 格式日志，便于链路追踪。

---

## 7. 代码质量保证 (Code Quality Assurance)

### 7.1 规范与检查
*   **Linting**: 启用 `clippy::pedantic` 级别检查，但在 CI 中允许部分非关键警告。
*   **Formatting**: 强制 `rustfmt` 检查。
*   **Git Hooks**: 提交前自动运行 `cargo check` 和 `cargo test`.

### 7.2 测试策略
*   **单元测试**: 覆盖所有 Service 层逻辑和 Utility 函数，覆盖率目标 > 80%。
*   **集成测试**: 使用 `sqlx::test` 在真实（Docker化）数据库环境中测试 Storage 层。
*   **E2E 测试**: 保留并扩展现有的 Python 脚本测试，覆盖核心业务流程。

---

## 8. 部署与运维方案 (Deployment & Operations Scheme)

### 8.1 容器化构建
使用多阶段构建减小镜像体积：
1.  **Builder 阶段**: 使用官方 Rust 镜像编译。
2.  **Runtime 阶段**: 使用 `gcr.io/distroless/cc-debian12`，仅包含二进制文件和必要的动态库，极大减小攻击面。

### 8.2 CI/CD 流水线
*   **Build**: 编译检查。
*   **Test**: 运行单元测试与集成测试。
*   **Audit**: 运行 `cargo audit` 检查依赖库漏洞。
*   **Deploy**: 自动构建 Docker 镜像并推送至仓库，触发 K8s 滚动更新。

### 8.3 容灾与恢复
*   **健康检查**: 提供 `/health` 接口，检查 DB 连接和 Redis 连接状态。
*   **优雅停机**: 监听 `SIGTERM` 信号，停止接收新请求，等待现有请求处理完毕（超时 30s）。

---

## 实施路线图 (Implementation Roadmap)

1.  **Phase 1 (Week 1)**: 安全加固（修复 P0 漏洞）、引入 `validator` 和 `sqlx` 迁移工具。
2.  **Phase 2 (Week 2)**: 数据库 Schema 规范化迁移、API 错误码体系重构。
3.  **Phase 3 (Week 3)**: 缓存层 (`Redis`) 接入、核心路径异步性能优化。
4.  **Phase 4 (Week 4)**: 完善测试覆盖率、部署 CI/CD 流水线、全链路压测。

**验收标准**:
*   所有 P0/P1 问题已解决。
*   `cargo audit` 无高危漏洞。
*   测试覆盖率 > 80%。
*   P99 延迟达标。
