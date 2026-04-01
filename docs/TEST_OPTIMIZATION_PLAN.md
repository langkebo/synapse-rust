# 测试优化方案

## 📊 当前状态分析

### 测试统计
- **总测试数**: 233个集成测试
- **测试通过率**: 99.6% (232/233)
- **测试类型分布**:
  - API测试: ~150个
  - 功能测试: ~50个
  - 性能测试: ~20个
  - 协议合规测试: ~13个

### 主要问题
1. **连接池超时**: 并发测试时数据库连接池可能耗尽
2. **缓存污染**: 测试间缓存状态未清理
3. **测试数据管理**: 缺乏统一的测试数据工厂
4. **性能监控**: 缺少测试性能指标

## 🎯 优化方案

### 1. 测试并行执行优化

#### 当前问题
```rust
// 当前配置：最大5个连接
sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)  // 并发测试时可能不足
```

#### 优化方案
```rust
// 优化配置：根据CPU核心数动态调整
let num_cpus = num_cpus::get();
let max_connections = (num_cpus * 2).min(20);

sqlx::postgres::PgPoolOptions::new()
    .max_connections(max_connections)
    .min_connections(2)
    .acquire_timeout(Duration::from_secs(15))
    .idle_timeout(Some(Duration::from_secs(300)))
    .max_lifetime(Some(Duration::from_secs(900)))
```

#### 实施步骤
1. ✅ 增加连接池大小
2. ✅ 添加连接池监控
3. ✅ 实现连接池预热

### 2. 测试缓存清理机制

#### 问题根源
```rust
// validate_token 会缓存 is_admin 状态3600秒
let cached = cache.get(&cache_key).await;
if cached.is_some() {
    return cached;
}
```

#### 解决方案
```rust
// 添加测试专用的缓存清理函数
#[cfg(test)]
pub fn clear_test_cache(cache: &CacheManager) {
    cache.clear_all();
}

// 在测试中使用
#[tokio::test]
async fn test_with_cache_cleanup() {
    let app = setup_test_app().await;
    clear_test_cache(&app.cache);
    // ... 测试代码
}
```

### 3. 测试数据管理优化

#### 当前问题
- 测试数据分散在各个测试文件中
- 缺乏统一的测试数据创建方式
- 测试数据清理不彻底

#### 解决方案：测试数据工厂

```rust
// tests/common/test_factory.rs
pub struct TestFactory {
    pool: Arc<PgPool>,
}

impl TestFactory {
    pub async fn create_user(&self, username: &str) -> TestUser {
        let user_id = format!("@{}:localhost", username);
        sqlx::query!(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3)",
            user_id, username, chrono::Utc::now().timestamp_millis()
        )
        .execute(&*self.pool)
        .await
        .unwrap();
        
        TestUser { user_id, username: username.to_string() }
    }
    
    pub async fn create_room(&self, creator: &str) -> TestRoom {
        // ... 房间创建逻辑
    }
    
    pub async fn cleanup(&self) {
        sqlx::query("TRUNCATE users, rooms CASCADE")
            .execute(&*self.pool)
            .await
            .unwrap();
    }
}
```

### 4. 测试性能监控

#### 实施方案
```rust
// tests/common/metrics.rs
use std::time::Instant;

pub struct TestMetrics {
    start_time: Instant,
    test_name: String,
}

impl TestMetrics {
    pub fn new(test_name: &str) -> Self {
        Self {
            start_time: Instant::now(),
            test_name: test_name.to_string(),
        }
    }
    
    pub fn report(&self) {
        let duration = self.start_time.elapsed();
        if duration > Duration::from_secs(5) {
            eprintln!("⚠️  Slow test: {} took {:?}", self.test_name, duration);
        }
    }
}

// 使用示例
#[tokio::test]
async fn test_example() {
    let metrics = TestMetrics::new("test_example");
    // ... 测试代码
    metrics.report();
}
```

### 5. 测试分类和标签

#### 实施方案
```rust
// 使用Cargo的特征来分类测试
#[cfg(feature = "slow-tests")]
#[tokio::test]
async fn slow_integration_test() {
    // 耗时较长的测试
}

#[cfg(feature = "quick-tests")]
#[tokio::test]
async fn quick_unit_test() {
    // 快速单元测试
}
```

#### 运行方式
```bash
# 只运行快速测试
cargo test --features quick-tests

# 运行所有测试
cargo test --all-features
```

## 📈 预期效果

### 性能提升
| 指标 | 当前 | 优化后 | 提升 |
|------|------|--------|------|
| 测试总耗时 | ~52秒 | ~35秒 | 33% |
| 连接池等待时间 | 5-10秒 | <1秒 | 90% |
| 测试稳定性 | 99.6% | 99.9% | 0.3% |
| 内存使用 | ~500MB | ~300MB | 40% |

### 可维护性提升
- ✅ 统一的测试数据创建方式
- ✅ 自动化的缓存清理
- ✅ 性能监控和告警
- ✅ 测试分类和标签

## 🚀 实施计划

### 阶段1：基础设施优化（第1周）
1. ✅ 优化数据库连接池配置
2. ✅ 添加连接池监控
3. ✅ 实现缓存清理机制

### 阶段2：测试数据管理（第2周）
1. ✅ 创建测试数据工厂
2. ✅ 统一测试辅助函数
3. ✅ 实现测试数据清理

### 阶段3：性能监控（第3周）
1. ✅ 添加测试性能指标
2. ✅ 实现慢测试告警
3. ✅ 生成测试报告

### 阶段4：测试分类（第4周）
1. ✅ 实现测试标签系统
2. ✅ 优化CI/CD流程
3. ✅ 文档更新

## 📝 最佳实践

### 1. 测试隔离
```rust
#[tokio::test]
async fn test_isolated() {
    let factory = TestFactory::new().await;
    
    // 每个测试都有独立的数据
    let user = factory.create_user("test").await;
    
    // 测试结束后自动清理
    factory.cleanup().await;
}
```

### 2. 避免缓存污染
```rust
#[tokio::test]
async fn test_with_fresh_cache() {
    let app = setup_test_app().await;
    
    // 清理缓存
    app.cache.clear_all().await;
    
    // 使用 get_admin_token 而不是手动设置 is_admin
    let admin_token = get_admin_token(&app).await;
    
    // ... 测试代码
}
```

### 3. 连接池管理
```rust
#[tokio::test]
async fn test_with_connection_pool() {
    let pool = get_test_pool().await.unwrap();
    
    // 检查连接池状态
    let status = pool.status();
    assert!(status.num_idle() > 0, "Connection pool exhausted");
    
    // ... 测试代码
}
```

## 🔍 监控指标

### 关键指标
1. **测试执行时间**: 每个测试的耗时
2. **数据库连接数**: 活跃连接和空闲连接数
3. **缓存命中率**: 缓存使用效率
4. **内存使用**: 测试过程中的内存峰值

### 告警阈值
- ⚠️  单个测试超过5秒
- ⚠️  连接池使用率超过80%
- ⚠️  内存使用超过1GB
- ⚠️  测试失败率超过1%

## 📚 参考资源

- [Rust测试最佳实践](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [SQLx连接池配置](https://docs.rs/sqlx/latest/sqlx/pool/struct.PoolOptions.html)
- [Tokio测试指南](https://tokio.rs/tokio/topics/testing)
