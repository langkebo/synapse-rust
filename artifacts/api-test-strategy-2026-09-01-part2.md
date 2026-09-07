# synapse-rust API 测试体系方案（Part 2）

**关联**: `api-test-strategy-2026-09-01-part1.md`（设计目标 / 金字塔 / 端点盘点 / 单元+集成+契约+E2E）
**版本**: 1.0
**作者**: API Testing Expert
**日期**: 2026-09-01

---

## 4.5 性能测试

**目标**: SLA 验证（p95 < 200ms / error < 0.1% / 10x 突发）

**工具**: `k6` (推荐) 或 `wrk` / `vegeta`

### 4.5.1 场景设计

| 场景 | 持续时间 | 并发 | 目标 RPS | 关键指标 |
|------|----------|------|----------|---------|
| **基准** | 5min | 50 | 100 RPS | p95 < 200ms |
| **峰值** | 2min | 500 | 1000 RPS | error < 1% |
| **突发** | 30s | 1000 | 5000 RPS | 不崩溃 |
| **持久** | 1h | 100 | 200 RPS | 无内存泄漏 |
| **同步压力** | 5min | 200 | sliding sync | p95 < 500ms |

### 4.5.2 样例 (`tests/perf/sliding-sync.js`)

```javascript
import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  stages: [
    { duration: '30s', target: 50 },
    { duration: '5m',  target: 200 },
    { duration: '30s', target: 0 },
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'],
    http_req_failed:   ['rate<0.001'],
  },
};

export default function() {
  const res = http.get(`${__ENV.BASE_URL}/_matrix/client/v3/sync?timeout=30000`, {
    headers: { 'Authorization': `Bearer ${__ENV.TOKEN}` },
  });
  check(res, { 'status 200': r => r.status === 200 });
  sleep(1);
}
```

## 4.6 安全测试

**目标**: OWASP API Security Top 10 覆盖

| OWASP 风险 | 测试方法 | 工具 |
|------------|----------|------|
| **API1: BOLA** | 跨用户访问 | 自研脚本 |
| **API2: 认证失效** | Token 过期、签名错误、伪造 | 自研 |
| **API3: BOPLA** | 管理员端点 user 角色调用 | 自研 |
| **API4: 资源耗尽** | 大 body / 大量房间 / 大量消息 | k6 + 自研 |
| **API5: 权限层级** | 普通用户调 admin | 自研（已覆盖） |
| **API6: 敏感业务流** | 注册轰炸、消息发送频率 | 自研 |
| **API7: SSRF** | Federation URL、Avatar URL | 手工 + 模糊 |
| **API8: 配置错误** | CORS / TLS / 错误信息泄露 | 半自动 |
| **API9: 库存管理** | 旧版本兼容 / 弃用端点 | 契约测试 |
| **API10: 不安全日志** | 敏感数据 (Token/密码) 进日志 | 静态分析 |

**新增脚本**: `tests/security/owasp_top10.sh`

## 🔄 五、CI/CD 集成

### 5.1 Pipeline 分层

```yaml
# .github/workflows/api-tests.yml (示意)
name: API Tests

on:
  pull_request:
    paths: ['src/**/*.rs', 'synapse-*/src/**']
  push:
    branches: [main]

jobs:
  unit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test --workspace --locked

  contract:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: pip install schemathesis
      - run: python tests/contract/run.py

  integration:
    runs-on: [self-hosted, synapse]
    services:
      postgres:
        image: postgres:16-alpine
      redis:
        image: redis:7-alpine
    steps:
      - run: ./tests/api/run.sh --profile core --junit junit.xml
      - uses: dorny/test-reporter@v1
        if: always()

  perf-nightly:
    runs-on: [self-hosted, synapse]
    if: github.event.schedule == 'nightly'
    steps:
      - run: ./tests/perf/run.sh --baseline
      - uses: actions/upload-artifact@v4
```

### 5.2 质量门禁

| 阶段 | 门禁 |
|------|------|
| **PR 提交** | unit + contract + integration_core 必须 100% 通过 |
| **Main 合并** | + integration_full 必须 95% 通过 |
| **Nightly** | + perf + security 全部通过；性能退化 > 10% 告警 |
| **Release** | 全量通过 + 安全扫描零 critical |

## 🛠 六、工具与基础设施

### 6.1 推荐工具栈

| 用途 | 工具 | 选型理由 |
|------|------|----------|
| 单元测试 | Rust 内置 `#[test]` | 无额外依赖 |
| 集成测试 | Bash + Python JSON | 项目已用，零新增 |
| 契约测试 | `schemathesis` | Python 原生，OpenAPI 3 完整支持 |
| 性能测试 | `k6` | Go 性能好，JS 脚本易写 |
| 安全扫描 | `owasp-zap` 或自研 fuzz | 重点是 BOLA / BOPLA |
| 覆盖率 | `cargo-tarpaulin` | 已有基线 |
| Mock | `wiremock` (HTTP) / `mockall` (Rust) | 隔离外部依赖 |
| 结果聚合 | `JUnit XML` + `dorny/test-reporter` | GitHub Actions 原生 |
| 趋势追踪 | InfluxDB + Grafana | 性能基线对比 |

### 6.2 测试数据管理

**策略**:

- **每个 test run 独立**: `RUN_ID=20260901_$(date +%s)_$$`
- **命名空间**: 测试用前缀 `test_xxx_${RUN_ID}`
- **清理**: 每次 run 结束自动清空测试数据（admin 端点）
- **夹具 (Fixture)**: 提取公共 setup（用户、房间、Token）到 `tests/fixtures/`

### 6.3 Mock 与隔离

**需要 Mock 的服务**:

- `matrix.org` 联邦密钥（沙箱环境）
- `APNs / FCM` 推送服务
- 外部 OIDC Provider
- Email / SMS 投递

**工具**: 本地 mock server（`wiremock`）启动在独立容器

## 📊 七、覆盖率与质量指标

### 7.1 必须指标

| 指标 | 目标 | 测量方法 |
|------|------|----------|
| **Endpoint 覆盖** | 95%+ | OpenAPI ↔ 测试用例对比 |
| **状态码覆盖** | 每个 endpoint 所有声明状态码 | 集成测试 |
| **错误码覆盖** | 每个业务错误码至少 1 触发 | 集成测试 |
| **角色覆盖** | user/admin/super_admin 每端点 | 集成测试 |
| **P0 性能** | p95 < 200ms | k6 |
| **错误率** | < 0.1% | k6 |
| **失败定位** | 自动报告 endpoint + input | 报告工具 |

### 7.2 报告样例

**每次 Run 输出**:

- `junit.xml` — CI 解析
- `coverage.html` — tarpaulin 覆盖
- `perf-report.html` — k6 dashboard
- `security-report.md` — OWASP 检查清单

## 📅 八、Roadmap（4 周落地）

### Week 1: 基础设施

- [ ] 建立 `tests/api/run.sh` 入口 + profile 切换
- [ ] 添加 `RUN_ID` 房间隔离（已完成）
- [ ] 集成 JUnit XML 输出
- [ ] OpenAPI 规范生成（`utoipa` 框架）— **前置** 契约测试

### Week 2: 增强集成层

- [ ] 拆分 core/full/nightly profile
- [ ] 补齐 18 个失败用例的测试逻辑
- [ ] 添加角色矩阵系统化测试
- [ ] 引入 `wiremock` 隔离外部依赖

### Week 3: 契约 + 性能

- [ ] OpenAPI 双向校验
- [ ] k6 性能基线（5 个核心场景）
- [ ] 性能趋势追踪

### Week 4: 安全 + CI

- [ ] OWASP Top 10 自动化检查
- [ ] CI 集成（PR / main / nightly）
- [ ] 报告 dashboard

## ⚠️ 九、风险与缓解

| 风险 | 影响 | 缓解策略 |
|------|------|----------|
| **OpenAPI 缺失** | 契约测试无法进行 | Week 1 优先 utoipa 生成 |
| **联邦测试需真实 mesh** | Federation 用例难 | mock matrix.org + wiremock |
| **E2EE 签名依赖** | Olm 库缺失 | 已知限制 — 文档化 |
| **服务时间敏感** | 状态码判断易误报 | 引入 wait_for_state 工具 |
| **Docker 重建慢** | 性能基线易受环境波动 | 容器化测试 runner + 冷/热分离 |

## 🎯 十、成功指标

本方案成功的判定标准：

- 4 周后，**集成测试通过率 ≥ 98%**（当前 79.9%）
- 6 周后，**契约测试覆盖 ≥ 90% endpoints**
- 8 周后，**性能基线建立且 CI 集成完成**
- 12 周后，**全量 PR 必跑 + Nightly 全量 + 报告 dashboard 在线**

---

**下一步行动**:

1. 评审本文档（特别是测试金字塔比例与 Week 1 任务）
2. 确认 Week 1 第一步：用 utoipa 提取 OpenAPI（最关键的路径依赖）
3. 决定性能基线是否上 InfluxDB + Grafana 或先用 k6 内置报告
4. 确认团队分工（哪几位 owner）
