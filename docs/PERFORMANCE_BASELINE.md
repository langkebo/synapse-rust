# 性能基准测试指南

> 日期：2026-04-04  
> 版本：v1.0  
> 工具：`benches/run_benchmarks.sh`, `scripts/generate_benchmark_data.sh`

---

## 一、概述

本指南说明如何运行性能基准测试、生成基准数据、解读测试结果和监控性能回归。

### 1.1 基准测试套件

| 套件 | 文件 | 测试内容 |
|------|------|---------|
| API 基准 | `performance_api_benchmarks.rs` | Matrix 客户端 API 性能 |
| Federation 基准 | `performance_federation_benchmarks.rs` | 联邦协议性能 |
| Database 基准 | `performance_database_benchmarks.rs` | 数据库查询性能 |

### 1.2 工具脚本

| 脚本 | 功能 |
|------|------|
| `benches/run_benchmarks.sh` | 运行基准测试，生成报告 |
| `scripts/generate_benchmark_data.sh` | 生成测试数据 |

---

## 二、快速开始

### 2.1 首次运行

```bash
# 1. 生成测试数据（小数据集）
bash scripts/generate_benchmark_data.sh preset small

# 2. 运行快速基准测试
bash benches/run_benchmarks.sh quick

# 3. 查看结果
ls benches/results/
```

### 2.2 完整基准测试

```bash
# 运行完整基准测试（更准确，耗时更长）
bash benches/run_benchmarks.sh full
```

---

## 三、生成测试数据

### 3.1 预设数据集

```bash
# 小数据集（开发/快速测试）
bash scripts/generate_benchmark_data.sh preset small
# 输出：1K users, 100 rooms, 10K events, 2K devices

# 中等数据集（CI/常规测试）
bash scripts/generate_benchmark_data.sh preset medium
# 输出：10K users, 1K rooms, 100K events, 20K devices

# 大数据集（性能压测）
bash scripts/generate_benchmark_data.sh preset large
# 输出：100K users, 10K rooms, 1M events, 200K devices
```

### 3.2 自定义数据生成

```bash
# 生成指定数量的用户
bash scripts/generate_benchmark_data.sh users 5000

# 生成指定数量的房间
bash scripts/generate_benchmark_data.sh rooms 500

# 生成指定数量的事件
bash scripts/generate_benchmark_data.sh events 50000

# 生成指定数量的设备
bash scripts/generate_benchmark_data.sh devices 10000
```

### 3.3 可重复性

所有数据生成使用固定随机种子（默认 42），确保结果可重复：

```bash
# 使用自定义种子
bash scripts/generate_benchmark_data.sh users 1000 123
```

### 3.4 清理测试数据

```bash
# 删除所有基准测试数据
bash scripts/generate_benchmark_data.sh cleanup
```

### 3.5 查看数据统计

```bash
# 显示当前测试数据统计
bash scripts/generate_benchmark_data.sh stats
```

---

## 四、运行基准测试

### 4.1 命令行模式

```bash
# 运行 API 基准测试
bash benches/run_benchmarks.sh api

# 运行 Federation 基准测试
bash benches/run_benchmarks.sh federation

# 运行 Database 基准测试
bash benches/run_benchmarks.sh database

# 运行所有基准测试
bash benches/run_benchmarks.sh all

# 快速模式（低精度，快速）
bash benches/run_benchmarks.sh quick

# 完整模式（高精度，慢）
bash benches/run_benchmarks.sh full
```

### 4.2 交互模式

```bash
bash benches/run_benchmarks.sh
```

然后选择菜单选项：
```
Select an option:
  1) Run API benchmarks
  2) Run federation benchmarks
  3) Run database benchmarks
  4) Run all Criterion benchmarks
  5) Run manual performance tests
  6) Run all performance suites
  7) Generate baseline report
  8) Compare results (baseline vs optimized)
  9) Quick benchmark run (fast mode)
  10) Full benchmark run (accurate mode)
  11) Setup benchmark data
  q) Quit
```

### 4.3 生成基准报告

```bash
# 生成当前基准报告
bash benches/run_benchmarks.sh baseline
```

输出示例：`benches/results/BASELINE_REPORT_20260404_120000.md`

---

## 五、数据库基准测试详解

### 5.1 测试覆盖

`performance_database_benchmarks.rs` 包含以下测试：

#### 用户查询基准
- `by_user_id`: 按 user_id 查询用户
- `by_username`: 按 username 查询用户

#### 房间查询基准
- `by_room_id`: 按 room_id 查询房间
- `public_rooms_list`: 查询公开房间列表

#### 事件查询基准
- `by_event_id`: 按 event_id 查询事件
- `by_room_recent`: 查询房间最近事件
- `by_room_time_range`: 按时间范围查询事件

#### 设备查询基准
- `by_user_id`: 查询用户所有设备
- `by_device_id`: 查询特定设备

#### 批量插入基准
- 测试不同批次大小（10, 50, 100）的插入性能

#### 索引效率基准
- 测试索引查询性能

### 5.2 运行数据库基准

```bash
# 确保有测试数据
bash scripts/generate_benchmark_data.sh preset small

# 设置数据库连接
export DATABASE_URL="postgresql://synapse:synapse@localhost:5432/synapse_test"

# 运行数据库基准
cargo bench --bench performance_database_benchmarks
```

### 5.3 解读结果

Criterion 输出示例：
```
database/user_query/by_user_id
                        time:   [1.2345 ms 1.2567 ms 1.2789 ms]
                        thrpt:  [782.15 elem/s 795.73 elem/s 809.31 elem/s]
```

- `time`: 平均执行时间及置信区间
- `thrpt`: 吞吐量（每秒操作数）

---

## 六、基准报告

### 6.1 基准报告结构

生成的基准报告包含：
1. 测试环境信息
2. 数据集统计
3. 各基准测试结果
4. 性能指标汇总

### 6.2 报告位置

```
benches/results/
├── BASELINE_REPORT_20260404_120000.md  # 基准报告
├── performance_api_benchmarks_output.txt
├── performance_federation_benchmarks_output.txt
├── performance_database_benchmarks_output.txt
└── ...
```

### 6.3 保存基准

```bash
# 保存当前基准作为参考
cp benches/results/BASELINE_REPORT_$(date +%Y%m%d).md \
   benches/results/baseline_reference.md
```

---

## 七、性能回归检测

### 7.1 建立基准

```bash
# 1. 在优化前运行完整基准
bash benches/run_benchmarks.sh full

# 2. 保存基准报告
cp benches/results/BASELINE_REPORT_*.md \
   benches/results/baseline_before_optimization.md
```

### 7.2 优化后对比

```bash
# 1. 应用优化

# 2. 运行基准测试
bash benches/run_benchmarks.sh full

# 3. 对比结果
# 手动对比两个报告文件
diff benches/results/baseline_before_optimization.md \
     benches/results/BASELINE_REPORT_*.md
```

### 7.3 性能回归阈值

建议阈值：
- **警告**：性能下降 > 5%
- **严重**：性能下降 > 10%
- **优化成功**：性能提升 > 5%

---

## 八、最佳实践

### 8.1 测试环境

1. **隔离环境**
   - 关闭其他应用
   - 使用专用测试数据库
   - 避免网络波动

2. **一致性**
   - 使用相同的数据集
   - 使用相同的硬件
   - 使用相同的配置

3. **可重复性**
   - 使用固定随机种子
   - 记录测试环境信息
   - 保存基准报告

### 8.2 测试频率

- **开发阶段**：快速模式，按需运行
- **PR 提交前**：完整模式，验证无回归
- **定期基准**：每月运行，建立趋势
- **重大优化后**：完整模式，验证效果

### 8.3 数据集选择

| 场景 | 推荐数据集 | 原因 |
|------|-----------|------|
| 本地开发 | small | 快速反馈 |
| CI 验证 | small/medium | 平衡速度和覆盖 |
| 性能优化 | medium/large | 真实负载 |
| 压力测试 | large | 极限场景 |

---

## 九、故障排查

### 9.1 常见问题

#### 问题 1：数据库连接失败

**错误**：
```
Failed to connect to database
```

**解决**：
```bash
# 检查数据库是否运行
docker compose -f docker/docker-compose.yml ps db

# 启动数据库
docker compose -f docker/docker-compose.yml up -d db

# 检查连接
psql -U synapse -d synapse_test -h localhost -c "SELECT 1"
```

#### 问题 2：测试数据不存在

**错误**：
```
No benchmark data found
```

**解决**：
```bash
# 生成测试数据
bash scripts/generate_benchmark_data.sh preset small
```

#### 问题 3：基准测试耗时过长

**解决**：
```bash
# 使用快速模式
bash benches/run_benchmarks.sh quick

# 或减少样本数
cargo bench --bench performance_database_benchmarks -- \
    --sample-size 10 --warm-up-time 1 --measurement-time 1
```

---

## 十、参考资料

- [Criterion.rs 文档](https://bheisler.github.io/criterion.rs/book/)
- [P2 长期改进计划](../synapse-rust/P2_LONG_TERM_IMPROVEMENT_PLAN.md)
- [数据库审计报告](../db/DATABASE_AUDIT_REPORT_2026-04-04.md)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：性能团队
