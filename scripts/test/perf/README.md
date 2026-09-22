# Matrix Core API Smoke Test

分层性能压测脚本套件，用于验证 Matrix 后端服务的核心 API 性能表现。

## 📋 测试场景

### 核心流程 (api_matrix_core.js)

1. **登录 (Login)** - 用户身份验证
2. **创建房间 (Create Room)** - 新建私密聊天室
3. **发送消息 (Send Message)** - 发送文本消息
4. **获取房间摘要 (Room Summary)** - 查询房间信息
5. **同步时间线 (Sync)** - 拉取最新事件

### 好友功能 (friend_search_and_list.js)

1. **好友搜索 (Friend Search)** - 按用户名/昵称模糊搜索
2. **好友列表分页 (Friend List Pagination)** - 分页获取好友列表

## 🚀 快速开始

### 前置条件

- **k6 已安装**: `brew install k6` (或从 [k6.io](https://k6.io/docs/getting-started/installation/) 下载)
- **Python 3.8+**: 用于生成报告
- **目标服务器可达**: 确保 `BASE_URL` 指向的 Matrix 服务器正在运行

### 基本用法

```bash
# 进入脚本目录
cd synapse-rust/scripts/test/perf/

# 运行烟雾测试 (默认 10 并发，30 秒)
./run_tests.sh smoke

# 运行完整测试套件
./run_tests.sh all
```

## 📊 测试类型

| 测试类型 | 并发用户 | 持续时间 | 目的 |
|---------|---------|---------|------|
| **Smoke** | 10 | 30s | 快速验证基本功能可用性 |
| **Baseline** | 50 | 60s | 建立性能基线 |
| **Stress** | 100 | 60s | 压力测试，寻找瓶颈 |
| **Peak** | 200 | 60s | 峰值负载测试 |
| **Soak** | 40 | 24h | 长时间稳定性测试 |
| **Friends** | 100 | 60s | 好友搜索专项测试 |

## ⚙️ 配置选项

### 环境变量

```bash
# 服务器配置
export BASE_URL="http://localhost:8008"
export ADMIN_USER="admin"
export ADMIN_PASS="Admin@123"

# 测试参数
export SOAK_VUS=40           # Soak 测试并发数
export SOAK_DURATION="24h"   # Soak 测试时长
export REQUEST_TIMEOUT=30000 # 请求超时 (ms)
```

### k6 命令行参数

```bash
# 自定义 VUS 和时长
k6 run --vus 20 --duration 60s api_matrix_core.js

# 导出详细结果
k6 run --summary-export results.json api_matrix_core.js

# 启用 JSON 日志
k6 run --out json=logs.json api_matrix_core.js
```

## 📈 性能阈值

### Smoke/Baseline 测试

| 指标 | P95 阈值 | 说明 |
|------|---------|------|
| Login | < 500ms | 登录响应时间 |
| Create Room | < 800ms | 创建房间时间 |
| Send Message | < 600ms | 发送消息时间 |
| Sync | < 1000ms | 同步时间线时间 |
| Room Summary | < 500ms | 获取房间摘要时间 |
| Error Rate | < 1% | 错误率 |

### Stress/Peak 测试

| 指标 | P95 阈值 |
|------|---------|
| Login | < 600ms |
| Create Room | < 1000ms |
| Send Message | < 800ms |
| Sync | < 1200ms |
| Room Summary | < 600ms |
| Error Rate | < 2% |

### Soak 测试

| 指标 | P95 阈值 |
|------|---------|
| Login | < 700ms |
| Create Room | < 1200ms |
| Send Message | < 900ms |
| Sync | < 1500ms |
| Room Summary | < 800ms |
| Error Rate | < 3% |

## 🔍 结果解读

### 输出文件

```
results/
├── smoke_results.json          # k6 摘要数据
├── smoke_output.log            # 控制台输出
├── smoke_details.json          # 详细日志 (可选)
└── performance_guardrail_report.md  # 评估报告
```

### 状态码含义

- ✅ **PASS**: 所有指标均在阈值范围内
- ❌ **FAIL**: 至少一个指标超出阈值
- ⚠️  **WARNING**: 部分指标缺失或无法解析

### 常见失败原因

| 现象 | 可能原因 | 解决方案 |
|------|---------|---------|
| 登录失败 | 认证服务不可用 | 检查服务器状态，确认 `/login` 端点正常 |
| 创建房间超时 | 数据库压力大 | 检查 DB 连接池，优化索引 |
| 错误率高 | 网络不稳定 | 检查网络延迟，增加重试机制 |
| P95 超标 | 资源不足 | 扩容 CPU/内存，优化热点代码 |

## 🛠️ 故障排查

### 服务器未响应

```bash
# 检查服务器状态
curl -v http://localhost:8008/_matrix/static/

# 验证认证端点
curl -X POST http://localhost:8008/_matrix/client/v3/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"admin"},"password":"Admin@123"}'
```

### k6 安装问题

```bash
# macOS
brew install k6

# Docker 运行
docker run --rm -i grafana/k6 run - <script.js
```

### Python 报告生成失败

```bash
# 检查 Python 版本
python3 --version  # 需要 >= 3.8

# 手动运行 guardrail
python3 guardrail.py --results-dir ./results --scenarios smoke
```

## 📝 自定义测试

### 添加新场景

在 `api_matrix_core.js` 中添加新的 group：

```javascript
group('Custom Feature', () => {
  const start = Date.now();
  const res = http.get(`${CONFIG.baseUrl}/custom/endpoint`, commonParams);
  const duration = Date.now() - start;
  
  customMetric.add(duration);
  
  check(res, {
    'custom feature ok': (r) => r.status === 200,
  });
});
```

### 调整阈值

编辑 `guardrail.py` 中的 `THRESHOLDS` 字典：

```python
THRESHOLDS["smoke"]["login_duration"] = 300  # 更严格的登录阈值
```

### 扩展测试步骤

修改 `run_tests.sh` 添加新的测试函数：

```bash
run_custom_test() {
    echo "Running Custom Test..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --vus 50 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/custom_results.json" \
        "$SCRIPT_DIR/custom_script.js"
}
```

## 🔗 相关文档

- [k6 官方文档](https://k6.io/docs/)
- [Matrix 协议规范](https://spec.matrix.org/v1.8/)
- [性能基准报告（已归档 2026-09-22，原 `scripts/load-test/` 已清理，备份在 `archive/load-test-2026-09-22/`）](../../archive/load-test-2026-09-22/PERFORMANCE_BASELINE.md)

## 📞 技术支持

如有问题，请联系：
- 后端团队
- DevOps 工程师

---

**最后更新**: 2026-09-22  
**版本**: 2.0.0
