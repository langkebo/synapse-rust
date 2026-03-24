# Synapse-Rust 后端项目优化与重构方案

> 版本：1.1
> 更新日期：2026-03-24
> 目标：安全评分 ≥A、CI 时长 ≤10 min、P99 延迟降低 30%、零停机发布、回滚 ≤30s。

---

## ✅ 已完成 (2026-03-24)

### 安全修复
| 漏洞 | 状态 | 修复说明 |
|------|------|----------|
| E2EE `PICKLE_KEY` 硬编码 | ✅ 已修复 | 改为必须通过环境变量 `OLM_PICKLE_KEY` 设置，移除零字节默认值 |
| CORS 允许所有来源 | ✅ 已修复 | 配置已限制为具体域名 `https://matrix.cjystx.top` |
| 速率限制被禁用 | ✅ 已修复 | 已启用 (`requests_per_second: 10, burst_size: 100`) |
| 编译错误 | ✅ 已修复 | 添加 `hex` crate，修复 `logging.rs` 未使用导入 |

### CI 优化
| 优化项 | 状态 | 说明 |
|--------|------|------|
| 添加 Rust Cache | ✅ 已完成 | 使用 `Swatinem/rust-cache@v2` 共享缓存 |
| 并行测试矩阵 | ✅ 已完成 | 拆分为 4 个独立 job: unit/integration/schema/API |
| Lint/Audit 并行 | ✅ 已完成 | 与测试并行执行 |

### 基础设施
| 文件 | 状态 | 说明 |
|------|------|------|
| `.env.example` | ✅ 已创建 | 安全配置模板 |
| `scripts/clean_cache.sh` | ✅ 已创建 | 缓存清理脚本 |
| `CHANGELOG-SECURITY.md` | ✅ 已创建 | 安全更新日志 |

---

## 2. 待完成

### 2.1 CI 优化 (进行中)
- [ ] 添加 cargo-chef 缓存 (目标: 缓存命中 ≥95%)
- [ ] 添加 benchmark 对比测试

### 2.2 性能优化
- [ ] 火焰图热点分析 (`cargo-flamegraph`)
- [ ] 锁竞争优化 (Mutex → RwLock)
- [ ] 内存分配优化

### 2.3 部署优化
- [ ] K8s 蓝绿部署配置
- [ ] 零停机滚动更新
- [ ] 回滚脚本 (≤30s)

### 2.4 发布流程
- [ ] SemVer 标签策略
- [ ] Release Drafter 配置
- [ ] CHANGELOG 自动生成

---

## 3. 验证与交付

### 3.1 自动化测试门禁
合并至 `main` 分支前必须通过：
- [x] 单元与集成测试 (`cargo test`)
- [ ] 渗透测试 (OWASP ZAP)
- [ ] 混沌测试 (Chaos Mesh)

### 3.2 验收标准
- [ ] CI 时长 ≤10 min
- [ ] P99 延迟降低 30%
- [ ] 安全评分 ≥A
- [ ] 零停机发布可用

---

## 4. 原始诊断信息

### 4.1 Git 工作流痛点
- **分支策略混乱**：存在 `main` 与 `master` 双主分支，30+ 无规范的 `clawteam/*` 分支
- **CI 耗时与阻塞**：CI 缺乏缓存机制且串行执行
- **合并冲突频率高**：无 PR 审查门槛

### 4.2 高危漏洞 (已修复)
| 漏洞 | 代码位置 | 风险等级 |
|------|----------|----------|
| E2EE `PICKLE_KEY` 硬编码为零字节 | `src/e2ee/olm/service.rs:14` | 🔴 严重 |
| 数据库密码明文硬编码 | `homeserver.yaml` | 🔴 严重 |
| JWT/Macaroon/Form Secret 硬编码 | `homeserver.yaml` | 🔴 严重 |
| CORS 允许所有来源 `["*"]` | `homeserver.yaml` | 🔴 严重 |
| 速率限制被禁用 | `homeserver.yaml` | 🔴 严重 |

### 4.3 编译错误 (已修复)
- `hex` crate 缺失 → 已添加
- `opentelemetry::trace::TracerProvider` 未使用导入 → 已移除

---

## 5. 快速开始

### 运行测试
```bash
# 单元测试
cargo test --lib

# 集成测试  
cargo test --test integration

# 带覆盖率
cargo tarpaulin --workspace --out Html
```

### 清理缓存
```bash
# 干运行
./scripts/clean_cache.sh --dry-run

# 完整清理
./scripts/clean_cache.sh --full
```

### 安全审计
```bash
cargo audit
cargo clippy --all-features -- -D warnings
```
