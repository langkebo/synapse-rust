# ROUND2-ISSUE-2: test_config 环境变量并行竞争（flaky test）

**Status**: 🟡 open
**Severity**: 中（CI 偶发失败，影响开发者信心）
**Discovered**: 三项目协同优化 阶段 A+B+C 验收 (2026-07-17)
**Origin**: `cargo test --workspace --lib` 偶发 1 个失败
**Blocks**: 不阻塞功能；影响 CI 信号稳定性

---

## 1. 背景

阶段 A+B+C 提交后运行 `cargo test --workspace --lib` 出现 1 个偶发失败：

```
---- test_config::tests::test_database_url_from_env stdout ----

thread 'test_config::tests::test_database_url_from_env' (144901945) panicked at src/test_config.rs:36:9:
assertion `left == right` failed
  left: "postgres://synapse:synapse@localhost:5432/synapse_test"
 right: "postgres://custom:custom@localhost:5432/custom"
```

**关键观察**：
- 单独运行 `cargo test -p synapse-rust --lib test_config` 时，**另一个测试** `test_database_url_default` 反而失败
- 使用 `--test-threads=1` 运行时，**全部 4 个测试通过**
- 失败的测试不固定（race condition 的典型特征）

## 2. 根因

源码 `src/test_config.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_url_default() {
        std::env::remove_var("TEST_DATABASE_URL");  // ← 删除 env var
        assert_eq!(test_database_url(), "postgres://synapse:synapse@localhost:5432/synapse_test");
    }

    #[test]
    fn test_database_url_from_env() {
        std::env::set_var("TEST_DATABASE_URL", "postgres://custom:custom@localhost:5432/custom");  // ← 设置 env var
        assert_eq!(test_database_url(), "postgres://custom:custom@localhost:5432/custom");
        std::env::remove_var("TEST_DATABASE_URL");
    }
}
```

**问题**：两个测试同时操作进程级全局状态 `TEST_DATABASE_URL` 环境变量，且未做任何同步。

**竞争时序**：

```
T1 (test_database_url_default)       T2 (test_database_url_from_env)
  remove_var("TEST_DATABASE_URL")
                                       set_var("TEST_DATABASE_URL", "custom")
  test_database_url()  ← 读到 "custom"（T2 设置的）
  assert_eq!(default)  ← 失败！
                                       test_database_url()  ← 读到 default（T1 已删除？或读到 custom）
                                       assert_eq!("custom")  ← 可能失败！
```

Rust 测试默认并行执行（多线程），`std::env::set_var` / `remove_var` 是**进程级**全局操作，因此任何两个测试若操作同一个 env var，必然竞争。

## 3. 影响范围

| 项 | 说明 |
|----|------|
| 复现概率 | ~50%（取决于线程调度） |
| CI 影响 | 间歇性失败，开发者可能误以为是真实回归 |
| 用户影响 | 无（仅测试代码） |
| 严重级别 | 中（噪声型 bug） |

## 4. 推荐修复方案

### 方案 A：使用 `serial_test` crate（推荐）

添加 `serial_test` dev-dependency，用 `#[serial]` 强制串行执行：

```toml
# Cargo.toml
[dev-dependencies]
serial_test = "3"
```

```rust
use serial_test::serial;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial]
    fn test_database_url_default() {
        std::env::remove_var("TEST_DATABASE_URL");
        assert_eq!(test_database_url(), "postgres://synapse:synapse@localhost:5432/synapse_test");
    }

    #[test]
    #[serial]
    fn test_database_url_from_env() {
        std::env::set_var("TEST_DATABASE_URL", "postgres://custom:custom@localhost:5432/custom");
        assert_eq!(test_database_url(), "postgres://custom:custom@localhost:5432/custom");
        std::env::remove_var("TEST_DATABASE_URL");
    }
}
```

**优点**：标准做法；扩展性好（其他 env-var 测试也可用）
**缺点**：新增一个 dev-dependency

### 方案 B：合并两个测试为一个

```rust
#[test]
fn test_database_url_env_handling() {
    // 保存原始状态
    let original = std::env::var("TEST_DATABASE_URL").ok();

    // 默认行为
    std::env::remove_var("TEST_DATABASE_URL");
    assert_eq!(test_database_url(), "postgres://synapse:synapse@localhost:5432/synapse_test");

    // env 覆盖行为
    std::env::set_var("TEST_DATABASE_URL", "postgres://custom:custom@localhost:5432/custom");
    assert_eq!(test_database_url(), "postgres://custom:custom@localhost:5432/custom");

    // 恢复
    match original {
        Some(v) => std::env::set_var("TEST_DATABASE_URL", v),
        None => std::env::remove_var("TEST_DATABASE_URL"),
    }
}
```

**优点**：无新依赖；逻辑紧凑
**缺点**：单测粒度变粗；若其他测试也读这个 env var 仍会竞争

### 方案 C：重构 `test_database_url()` 改为显式注入参数

将 env 读取移出函数本身，由调用方注入：

```rust
pub fn database_url_from(env_value: Option<&str>) -> String {
    env_value
        .map(|s| s.to_string())
        .unwrap_or_else(|| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string())
}

pub fn test_database_url() -> String {
    database_url_from(std::env::var("TEST_DATABASE_URL").ok().as_deref())
}
```

测试时直接调用 `database_url_from(Some("..."))` / `database_url_from(None)`，不操作 env var。

**优点**：彻底消除 env 依赖；函数更纯；测试更稳定
**缺点**：需修改生产代码（虽然只是 refactor）

## 5. 验收

- [ ] 选定方案并实施
- [ ] `cargo test -p synapse-rust --lib test_config` 10 次连续运行全通过
- [ ] `cargo test --workspace --lib` 10 次连续运行无 env 相关失败

## 6. 工时估计

| 方案 | 时间 |
| ------ | ------ |
| A（serial_test） | 0.1 天 |
| B（合并测试） | 0.05 天 |
| C（重构注入） | 0.2 天 |

## 7. 备注

- 这是经典的 Rust 测试反模式（process-global state mutation in parallel tests）
- 同类问题可能在其他测试文件中也存在，建议修复时全局 grep `std::env::set_var` / `std::env::remove_var` 在 `#[cfg(test)]` 模块中的使用
- 推荐采用**方案 A**（serial_test）作为最小侵入修复，方案 C 作为长期改进
- 全仓 grep 命令：`rg "std::env::(set_var|remove_var)" --type rust -l` 查找所有可能受影响的文件
