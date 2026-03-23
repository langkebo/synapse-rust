# synapse-rust 优化方案

> 基于代码审计结果制定
> 生成日期: 2026-03-23

---

## 一、安全优化

### 1.1 CSRF 防护中间件 ❌ 缺失

**问题**: 当前项目缺少 CSRF 防护机制

**方案**:
```rust
// src/web/middleware/csrf.rs
pub struct CsrfMiddleware {
    pub allowed_methods: Vec<Method>,
}

impl<S> Handler<S> for CsrfMiddleware {
    async fn handle(&self, cx: Context<S>) -> Response {
        // 检查请求来源
        // 验证 CSRF token
    }
}
```

**优先级**: P0 (高)
**工作量**: 2 小时

---

### 1.2 XSS 防护增强 ⚠️ 部分

**问题**: Matrix 事件内容缺少净化

**方案**:
```rust
// src/common/sanitizer.rs
pub struct ContentSanitizer;

impl ContentSanitizer {
    pub fn sanitize_event_content(content: &Value) -> Value {
        // 移除危险 HTML 标签和属性
        // 净化用户输入
    }
}
```

**优先级**: P1 (中)
**工作量**: 4 小时

---

### 1.3 中心化输入验证 ⚠️ 分散

**问题**: 输入验证分散在各个路由中

**方案**:
```rust
// src/web/middleware/validation.rs
pub fn validate_user_input<T: DeserializeOwned>(cx: &mut Context) -> Result<T, ApiError> {
    // 统一验证逻辑
    // 长度限制
    // 格式验证
}
```

**优先级**: P1 (中)
**工作量**: 6 小时

---

## 二、性能优化

### 2.1 数据库查询超时 ⚠️ 缺失

**问题**: SQL 查询未设置超时

**方案**:
```rust
// src/storage/mod.rs
pub async fn query_with_timeout<T>(
    query: Query<'_, Pool, SqliteArguments<'_>>,
    timeout: Duration,
) -> Result<T, sqlx::Error> {
    tokio::time::timeout(timeout, query.fetch_one(&pool)).await?
}
```

**优先级**: P0 (高)
**工作量**: 2 小时

---

### 2.2 慢查询监控 ⚠️ 缺失

**问题**: 无法追踪慢查询

**方案**:
```rust
// 启用 PostgreSQL 慢查询日志
ALTER SYSTEM SET log_min_duration_statement = 1000;

// 添加查询日志中间件
```

**优先级**: P1 (中)
**工作量**: 1 小时

---

### 2.3 缓存配置优化 ⚠️ 可调整

**问题**: 默认配置可能不适合高负载场景

**方案**:
```rust
// src/cache/config.rs
impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 100_000,      // 从 50_000 提升
            time_to_live: 7200,         // 从 3600 提升
            ..Default::default()
        }
    }
}
```

**优先级**: P2 (低)
**工作量**: 0.5 小时

---

## 三、架构优化

### 3.1 移除 glob re-exports ⚠️ 命名空间污染

**问题**: lib.rs 使用大量 `#[allow(ambiguous_glob_reexports)]`

**方案**:
```rust
// lib.rs - 改进导出
pub mod auth { pub use crate::auth::*; }  // 移除 allow
// 改为显式导出
pub use crate::auth::jwt::JwtService;
pub use crate::auth::password::PasswordService;
```

**优先级**: P2 (低)
**工作量**: 4 小时

---

### 3.2 测试覆盖率提升 📈 20%+ → 80%+

**当前状态**: 20+ 测试文件

**方案**:
1. 核心模块单元测试
2. 集成测试
3. API 端到端测试

**优先级**: P1 (中)
**工作量**: 20+ 小时

---

### 3.3 完善 Rustdoc 文档 📝 不足

**方案**:
- 为所有 public API 添加文档注释
- 添加使用示例

**优先级**: P2 (低)
**工作量**: 8 小时

---

## 四、优化实施计划

### 第一阶段: 安全修复 (P0)
| 任务 | 工作量 | 状态 |
|------|--------|------|
| CSRF 中间件 | 2h | - |
| 查询超时 | 2h | ✅ 已完成 |

### 第二阶段: 性能优化 (P1)
| 任务 | 工作量 | 状态 |
|------|--------|------|
| 慢查询监控 | 1h | - |
| 缓存优化 | 0.5h | ✅ 已完成 |
| 输入验证 | 6h | - |

### 第三阶段: 架构改进 (P2)
| 任务 | 工作量 | 状态 |
|------|--------|------|
| 移除 glob exports | 4h | - |
| 测试覆盖率 | 20h | - |
| Rustdoc | 8h | - |

---

## 五、验收标准

- [ ] CSRF 防护生效
- [ ] 所有查询有超时保护
- [ ] 慢查询可追踪
- [ ] 测试覆盖率 ≥ 80%
- [ ] 无 glob re-exports 警告
