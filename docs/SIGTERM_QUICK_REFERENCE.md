# SIGTERM 内存问题 - 快速参考

## 🚨 立即行动（看到 SIGTERM 时）

```bash
# 方案 A: 最保守（串行运行）
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils --test-threads 1
```

```bash
# 方案 B: 分 crate 测试
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-services --lib --features test-utils
```

```bash
# 方案 C: 单个测试用例
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"
PATH="/usr/bin:/bin:$PATH" cargo test -p synapse-web --lib --features test-utils -- media_route --nocapture
```

## 📊 内存占用估算

| 场景 | 估算内存 |
|------|---------|
| 单 crate 测试 (1 线程) | 800MB - 1.5GB |
| 多 crate 测试 (4 线程) | 3GB - 5GB |
| Workspace 级测试 | 5GB+ ⚠️ 必然 OOM |
| 编译阶段 (debug) | 2GB - 4GB |

## 🔧 环境变量

```bash
# 必须设置
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false,muzzy_decay_ms:1000"

# 有助于调试
export RUST_BACKTRACE=1
export TEST_DB_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
```

## ❌ 避免操作

- ❌ `cargo nextest run --workspace ...` (工作区级命令)
- ❌ `--test-threads 4` (在工作区环境)
- ❌ 并行运行多个 cargo 命令
- ❌ 在 work 树中运行 `cargo clean`

## ✅ 推荐操作

- ✅ `-p <crate>` 限定单个 crate
- ✅ `--test-threads 1` 串行运行
- ✅ 每个 crate 测试完成后删除 target 目录
- ✅ 使用 `cargo nextest` 而非 `cargo test`

## 📚 相关文档

- [SIGTERM_ROOT_CAUSE_ANALYSIS_2026-09-23.md](./SIGTERM_ROOT_CAUSE_ANALYSIS_2026-09-23.md) - 完整分析报告
- [docs/audit/AUDIT_SUMMARY_2026-09-12.md](./AUDIT_SUMMARY_2026-09-12.md) - Gate 漂移分析

## 🆘 紧急联系

如遇到持续问题：
1. 检查 WorkBuddy 内存限制
2. 尝试在 Docker 中运行
3. 查看 `~/.workbuddy/MEMORY.md` 中的历史解决方案

---

*更新时间：2026-09-23*
