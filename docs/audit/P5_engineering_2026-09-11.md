# P5 — 工程质量与防复发

> **审查日期**: 2026-09-11
> **基线**: `506e41a6`（迁移/部署三项修复后），工作树对本次审查干净
> **范围**: lint 门禁真实性、CI blocking 有效性、死代码、仓库治理、god-file
> **方法**: 先实测门禁"是否真的在验证东西"，再处理能安全闭环的项；不能安全闭环的如实记录边界

---

## 0. 结论摘要

| 状态 | 数量 | 项 |
|---|---|---|
| ✅ **已修复** | 4 | doc-test 空门禁 · `--tests` 构建被 clippy 拒绝 · `1necho` · 空目录 |
| 🟡 已量化并移交 | 3 | clippy cosmetic 警告 15 条 · `cargo doc` 3.5k 警告 · 157 处 `allow(dead_code)` |
| ⚪ 未测（如实标注） | 3 | god-file 漂移 · mock 漂移 · 构建产物治理 |

> **P5 的核心判断：仓库的"门禁数量"远大于"门禁效力"。**
> 15 个 workflow、多道 lint/测试步骤，但其中至少 2 道（doc-test、synapse-federation 的 `--tests`）
> 处于"看着绿、实际什么都没验证或本该失败"的状态。下面每一项都附实测证据。

---

## 1. ✅ 已修复

### 1.1 doc-test 门禁是空门禁，且曾放过一个真实编译错误

**缺陷**（`ci.yml` 原 `Run doc tests` 步骤）：
```yaml
- name: Run doc tests
  run: cargo test --doc --locked          # 只编译根 package
```
根 package `synapse-rust` 有 **0 个 doc test** ⇒ 该步骤永远输出 `running 0 tests` 并通过。

**危害已实证**：该门禁曾以全绿状态放过一个 rustdoc 专属编译错误 ——
本会话 P0 复核期间由 `--workspace` 首次暴露：

```
error[E0106]: missing lifetime specifiers
   --> src/federation/edu.rs:562:74
562 | fn validate_profile_update_content(edu: &Value, origin: &str) -> Option<(&str, Option<&str>, Option<&str>)>
```

**为何其它门禁都测不到**：返回值的生命周期从未被校验，
`cargo build` / `cargo check` / `cargo clippy` 均不报错，**只有 rustdoc 编译会报**。

**对照实验**：

| 命令 | 结果 |
|---|---|
| `cargo test --doc --locked`（原 CI 口径） | `EXIT=0`，**0 tests** |
| `cargo test --doc --locked --workspace`（修复后） | `EXIT=0`，真实编译各 crate 的 doc |

**修复**：改为 `--workspace`，并在 workflow 注释中写明捕获对象与**已知局限**。

**⚠️ 不得误认为这是 doc-test 覆盖率**：全 workspace 仅 4 个 doc test，且**全部 `#[ignore]`d**，
没有任何 doc 示例被真正执行。这是一道 **rustdoc 编译门禁**，不是执行覆盖。

### 1.2 `synapse-federation` 的 `--tests` 构建被 clippy 拒绝

**缺陷**（由 `cargo clippy --workspace --all-targets --all-features` 暴露）：
```
error: `panic` should not be present in production code
   --> synapse-federation/src/edu.rs:258
error: could not compile `synapse-federation` (lib test)
```
即**该 crate 的测试构建无法通过自身的 lint 配置**。

**CI 未捕获的两个叠加原因**：
1. `ci.yml` 的 `Run clippy` 只检 lib 范围（不含 `#[cfg(test)]` 块）
2. 唯一的测试范围 clippy step 只覆盖 `-p synapse-services`（workflow 注释自述为
   "避免整个 workspace test 编译时间爆炸"），`synapse-federation` 不在其中

**根因**：`synapse-federation/Cargo.toml` 设 `panic = "deny"`，而 lib.rs 的测试豁免为
`#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]` —— **不含 `panic`**。
且 `expect_used = "deny"` 同样无豁免 ⇒ 该 crate 的测试**不能用 panic!/expect/unwrap 中任何一种**。

**修复**：改为 `Result` 返回型测试（Rust 测试的标准零 panic 惯用法）：
```rust
fn test_edu_type_display_matches_from_str() -> Result<(), UnknownEduType> {
    ...
    let parsed = EduType::from_str(&s)?;   // ? 传播，失败信息不损失
    Ok(())
}
```

**证据**：

| 命令 | 修复前 | 修复后 |
|---|---|---|
| `cargo clippy -p synapse-federation --tests -- -D warnings` | 编译失败 | ✅ `EXIT=0` |
| `cargo clippy --workspace --all-targets --all-features` | ❌ `EXIT=101` | ✅ **`EXIT=0`**（0 编译错误） |
| `cargo nextest -p synapse-federation` | 182 passed | ✅ 182 passed |

### 1.3 仓库治理：stray 文件与空目录

| 项 | 处理 |
|---|---|
| 根目录 `1necho`（0 字节，**已被 git 跟踪**，疑似 `echo > 1necho` 手滑） | 已 `git rm` |
| `synapse-cache/src/cache/` 空目录（`lib.rs` 无 `mod cache` 引用） | 已删除（git 不跟踪空目录，仅工作区清理） |

> 说明：仓库唯一的另一处 0 字节跟踪文件是 `tests/.gitkeep`，属**有意保留**，未触碰。

---

## 2. 🟡 已量化，移交后续

### 2.1 clippy cosmetic 警告 15 条（test 构建）

workspace `--all-targets` 下的非阻塞警告：

| crate | 数量 | lint |
|---|---|---|
| `synapse-e2ee` (lib test) | 7 | `bool_assert_comparison`（`assert_eq!(x, true)`）等 |
| `synapse-storage` (lib test) | 4 | `field_reassign_with_default` 等 |
| `synapse-cache` (lib test) | 1 | — |

全部可用 `cargo clippy --fix --lib -p <crate> --tests` 自动修，**不影响编译**。
是否把它们纳入 CI（即扩大 clippy 测试范围到全 workspace）需要权衡 CI 时长，
**本次未擅自扩大门禁范围**。

### 2.2 `cargo doc` 约 3,500 条 unresolved intra-doc-link 警告

`cargo doc --no-deps --workspace` → `EXIT=0` 但 **3,487 条 warning**，主要是
`unresolved link to \`new\`` / `\`get_stats\`` 等简写 intra-doc link。

⇒ **不能**直接把 `cargo doc --workspace -- -D warnings` 作为门禁（会立刻爆红）。
若要治理，应先批量修正为完整路径（`[\`new\`]: Self::new` 之类），再启用门禁。

### 2.3 157 处 `#[allow(dead_code)]` / `#[allow(unused*)]`

规模本身是死代码债的输入。P1 阶段抽查未发现被豁免的**安全校验**函数，
但未逐一核查 ⇒ 保留为待办。

---

## 3. ⚪ 未测（如实标注，不作为结论）

以下项属 P5 范围但**本会话未实测**，不得引用为结论：

1. **god-file 漂移**：已知 `synapse-storage/src/room/mod.rs` 2,270 行、
   `device/mod.rs` 2,192 行、`refresh_token/mod.rs` 2,186 行；
   未评估拆分收益与风险。
2. **mock 漂移**：`test_mocks` 与真实实现的行为等价性未验证
   （这是 false-green 的常见来源）。
3. **构建产物治理**：`target/` 92G + `docker/deploy` 18G + `target_amd64` 4.0G
   + `target_arm64` 3.8G ≈ **118G**；未处理（属环境清理，且 `docker/deploy` 可能含部署数据，
   贸然清理有风险）。

---

## 4. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 1) doc-test 门禁：根级 vs workspace 级的对照
cargo test --doc --locked              # 0 tests —— 原 CI 口径，什么也不验证
cargo test --doc --locked --workspace  # 真实编译各 crate doc

# 2) clippy 测试范围盲区
cargo clippy --all-features --locked -- -D warnings                      # CI 口径，绿
cargo clippy --workspace --all-targets --all-features --locked           # 暴露 test 构建问题
cargo clippy -p synapse-federation --all-features --tests --locked -- -D warnings  # 曾编译失败

# 3) 治理项
git ls-files 1necho                    # 修复前有输出，现在为空
find synapse-cache/src -type d -empty  # 修复前有 synapse-cache/src/cache

# 4) 量化残留
cargo doc --no-deps --workspace 2>&1 | grep -c '^warning'    # ~3487
grep -rn '#\[allow(dead_code)\]\|#\[allow(unused' --include='*.rs' src/ synapse-*/src/ | wc -l  # 157
```

---

## 5. 移交后续阶段

| 项 | 目标 |
|---|---|
| 是否将 clippy 测试范围扩到全 workspace（需权衡 CI 时长） | **P5 后续 / 用户决策** |
| `cargo doc` 3.5k intra-doc-link 警告治理后再启用门禁 | **P5 后续** |
| 157 处 `allow(dead_code)` 抽查是否存在未接线逻辑 | **P5 后续** |
| god-file 拆分评估、mock 漂移核查 | **P5 后续** |
| 118G 构建产物治理 | **P5 后续（环境清理）** |
