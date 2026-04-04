# 测试执行建议与下一步行动

> 日期：2026-04-03  
> 文档类型：执行建议  
> 说明：基于当前项目状态，提供测试执行和能力升级的具体建议

---

## 一、当前状态总结

### 1.1 已完成工作

**文档治理（P0）**：✅ 100% 完成
- 删除失真文档
- 统一迁移入口
- 明确测试门禁
- 收敛对外口径
- 建立文档索引

**能力补证（P1）**：✅ 100% 完成
- 建立四个核心能力域的能力矩阵
- 建立最小互操作验证清单
- 完成验证证据映射

**测试代码**：✅ 100% 完成
- AppService 集成测试（5 个测试）
- Federation 互操作测试（Docker Compose + 自动化脚本）
- 修复 admin 注册流程

**架构评估（P2）**：✅ 已完成
- 评估 P2 任务可行性
- 确定优先级和推荐方案

### 1.2 当前瓶颈

**测试执行**：⏳ 0% 完成
- AppService 集成测试：代码已完成，待 CI 执行
- Federation 互操作测试：实施已完成，待手动执行

---

## 二、测试执行建议

### 2.1 Federation 互操作测试

**优先级**：🔴 高

**执行方式**：手动执行（推荐）

**原因**：
1. 测试需要构建 Docker 镜像（首次约 10-15 分钟）
2. 需要启动 6 个容器（2 个 homeserver + 2 个数据库 + 2 个 Redis）
3. 测试过程需要 3-5 分钟
4. 适合在独立终端会话中执行

**执行步骤**：
```bash
# 1. 确认当前目录
cd /Users/ljf/Desktop/hu/synapse-rust

# 2. 检查端口是否被占用
lsof -i :8008 -i :8009 -i :8448 -i :8449

# 3. 如果端口被占用，停止现有服务
docker-compose down

# 4. 执行 Federation 测试
./tests/federation_interop_test.sh
```

**详细指南**：`FEDERATION_TEST_EXECUTION_GUIDE_2026-04-03.md`

**预期结果**：
- ✅ 所有 10 个测试点通过
- ✅ 跨服务器邀请、加入、消息同步全部成功

**如果测试通过**：
1. 更新 `CAPABILITY_STATUS_BASELINE_2026-04-02.md`
2. 将 Federation 从"部分实现"升级为"已实现并验证（基础闭环）"
3. 更新 `FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`

### 2.2 AppService 集成测试

**优先级**：🟡 中高

**执行方式**：CI 自动执行（推荐）

**原因**：
1. 测试依赖正确配置的测试数据库环境
2. 本地测试环境存在数据库初始化问题
3. CI 环境已配置完整的测试基础设施

**执行步骤**：
```bash
# 方式 1：触发 GitHub Actions（推荐）
git add tests/integration/api_appservice_*.rs
git commit -m "Add AppService integration tests"
git push

# 方式 2：本地尝试（可能失败）
cargo test --test api_appservice_tests
cargo test --test api_appservice_basic_tests
```

**详细指南**：`APPSERVICE_CI_EXECUTION_GUIDE_2026-04-03.md`

**预期结果**：
- ✅ 5 个测试全部通过
- ✅ 注册/查询闭环、虚拟用户闭环验证成功

**如果测试通过**：
1. 更新 `CAPABILITY_STATUS_BASELINE_2026-04-02.md`
2. 将 AppService 从"部分实现"升级为"已实现并验证（最小闭环）"
3. 更新 `APPSERVICE_VERIFICATION_MAPPING_2026-04-03.md`

---

## 三、推荐执行顺序

### 3.1 短期（今天）

**第一步：执行 Federation 互操作测试**
- 时间：约 15-20 分钟（首次）
- 风险：低（独立环境，不影响现有服务）
- 收益：高（验证核心 Federation 功能）

**第二步：提交 AppService 测试代码**
- 时间：约 5 分钟
- 风险：低（仅提交代码，CI 自动执行）
- 收益：高（触发 CI 测试）

**第三步：根据测试结果更新文档**
- 时间：约 10-15 分钟
- 风险：无
- 收益：高（反映最新验证状态）

### 3.2 中期（本周）

**如果 Federation 测试通过**：
1. 更新能力基线文档
2. 更新验证映射文档
3. 更新 backlog 执行状态

**如果 AppService 测试通过**：
1. 更新能力基线文档
2. 更新验证映射文档
3. 更新 backlog 执行状态

**如果测试失败**：
1. 分析失败原因
2. 修复问题
3. 重新执行测试

### 3.3 长期（下月）

**可选优化项**：
1. 考虑 P2-4（性能基线）
2. 评估 P2-2（路由拆分）
3. 调试本地测试环境（可选）

---

## 四、能力状态升级路径

### 4.1 当前能力状态

| 能力域 | 当前状态 | 验证覆盖 |
|--------|----------|----------|
| E2EE | 已实现并验证（基础闭环） | ✅ 充分 |
| Admin | 已实现并验证（最小闭环） | ✅ 充分 |
| Federation | 部分实现（验证覆盖已明确） | ⏳ 待执行互操作测试 |
| AppService | 部分实现（验证覆盖已明确） | ⏳ 待执行集成测试 |

### 4.2 升级路径

**Federation**：
- 当前：部分实现（验证覆盖已明确）
- 条件：互操作测试全部通过
- 目标：已实现并验证（基础闭环）

**AppService**：
- 当前：部分实现（验证覆盖已明确）
- 条件：集成测试全部通过
- 目标：已实现并验证（最小闭环）

### 4.3 升级后的能力状态

| 能力域 | 升级后状态 | 验证强度 |
|--------|------------|----------|
| E2EE | 已实现并验证（基础闭环） | 高 |
| Admin | 已实现并验证（最小闭环） | 高 |
| Federation | 已实现并验证（基础闭环） | 高 |
| AppService | 已实现并验证（最小闭环） | 高 |

---

## 五、风险评估

### 5.1 Federation 测试风险

**技术风险**：🟢 低
- Docker 环境隔离，不影响现有服务
- 测试脚本有完整的清理机制
- 失败不会影响代码库

**时间风险**：🟡 中
- 首次构建镜像需要 10-15 分钟
- 后续执行只需 3-5 分钟

**资源风险**：🟢 低
- 需要约 4GB 内存
- 需要约 10GB 磁盘空间
- 测试完成后自动清理

### 5.2 AppService 测试风险

**技术风险**：🟡 中
- 本地测试环境存在数据库初始化问题
- CI 环境应该正常

**时间风险**：🟢 低
- CI 自动执行，无需等待
- 本地执行约 1-2 分钟（如果环境正常）

**资源风险**：🟢 低
- 测试使用隔离的测试数据库
- 不影响生产数据

---

## 六、故障应对方案

### 6.1 Federation 测试失败

**可能原因**：
1. Docker 镜像构建失败
2. 端口被占用
3. 服务健康检查失败
4. 跨服务器通信失败

**应对方案**：
1. 查看详细错误日志
2. 参考 `FEDERATION_TEST_EXECUTION_GUIDE_2026-04-03.md` 故障排查章节
3. 修复问题后重新执行
4. 如果问题复杂，记录问题并延后处理

### 6.2 AppService 测试失败

**可能原因**：
1. 测试数据库配置问题
2. Admin 注册流程问题
3. AppService 路由问题

**应对方案**：
1. 查看 CI 日志
2. 本地调试（如果环境正常）
3. 修复问题后重新提交
4. 如果是环境问题，在 CI 中验证

---

## 七、成功标准

### 7.1 Federation 测试成功标准

```
==========================================
Test Summary
==========================================
Passed: 10
Failed: 0

All tests passed!
```

**验证点**：
- ✅ 两个 homeserver 成功启动
- ✅ 用户注册成功
- ✅ 跨服务器邀请成功
- ✅ 跨服务器加入成功
- ✅ 消息同步成功
- ✅ 双向消息传递成功

### 7.2 AppService 测试成功标准

```
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**验证点**：
- ✅ AppService 列表查询成功
- ✅ AppService 注册成功
- ✅ AppService 查询成功
- ✅ 虚拟用户创建成功
- ✅ 路由存在性验证成功

---

## 八、文档更新清单

### 8.1 如果 Federation 测试通过

需要更新的文档：
1. `CAPABILITY_STATUS_BASELINE_2026-04-02.md`
   - Federation 状态：部分实现 → 已实现并验证（基础闭环）

2. `FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`
   - 更新互操作测试状态：待执行 → 已通过

3. `BACKLOG_EXECUTION_STATUS_2026-04-03.md`
   - 更新测试执行完成度：0/2 → 1/2

4. `OPTIMIZATION_SUMMARY_2026-04-03.md`
   - 添加 Federation 测试执行结果

### 8.2 如果 AppService 测试通过

需要更新的文档：
1. `CAPABILITY_STATUS_BASELINE_2026-04-02.md`
   - AppService 状态：部分实现 → 已实现并验证（最小闭环）

2. `APPSERVICE_VERIFICATION_MAPPING_2026-04-03.md`
   - 更新集成测试状态：待执行 → 已通过

3. `BACKLOG_EXECUTION_STATUS_2026-04-03.md`
   - 更新测试执行完成度：1/2 → 2/2

4. `OPTIMIZATION_SUMMARY_2026-04-03.md`
   - 添加 AppService 测试执行结果

### 8.3 如果两个测试都通过

额外更新：
1. `FINAL_STATUS_REPORT_2026-04-03.md`
   - 更新测试执行完成度
   - 更新能力状态总结
   - 更新项目成熟度评估

2. `PROJECT_REVIEW_INDEX_2026-04-03.md`
   - 添加测试执行指南的引用

---

## 九、总结

### 9.1 当前最优行动路径

1. **立即执行**：Federation 互操作测试
   - 命令：`./tests/federation_interop_test.sh`
   - 时间：15-20 分钟
   - 收益：验证核心 Federation 功能

2. **随后执行**：提交 AppService 测试代码
   - 命令：`git add tests/integration/api_appservice_*.rs && git commit && git push`
   - 时间：5 分钟
   - 收益：触发 CI 自动测试

3. **根据结果**：更新能力基线文档
   - 时间：10-15 分钟
   - 收益：反映最新验证状态

### 9.2 预期成果

**如果测试全部通过**：
- ✅ 四个核心能力域全部达到"已实现并验证"状态
- ✅ 验证证据补充工作 100% 完成
- ✅ 项目成熟度显著提升

**项目状态**：
- 文档治理：✅ 已完成
- 能力补证：✅ 已完成
- 验证证据：✅ 已完成
- 架构收口：⏸️ 可选（等待验证证据充分后再考虑）

### 9.3 下一阶段工作

**验证证据充分后**：
1. 考虑 P2-2（路由拆分）
2. 考虑 P2-4（性能基线）
3. 持续维护文档索引和能力基线

---

## 十、快速参考

### 10.1 关键命令

```bash
# Federation 测试
./tests/federation_interop_test.sh

# AppService 测试（本地）
cargo test --test api_appservice_tests

# 提交代码触发 CI
git add tests/integration/api_appservice_*.rs
git commit -m "Add AppService integration tests"
git push

# 查看 Docker 日志
docker-compose -f docker-compose.federation-test.yml logs -f
```

### 10.2 关键文档

- **Federation 测试指南**：`FEDERATION_TEST_EXECUTION_GUIDE_2026-04-03.md`
- **AppService CI 指南**：`APPSERVICE_CI_EXECUTION_GUIDE_2026-04-03.md`
- **能力基线**：`CAPABILITY_STATUS_BASELINE_2026-04-02.md`
- **执行状态**：`BACKLOG_EXECUTION_STATUS_2026-04-03.md`
- **最终报告**：`FINAL_STATUS_REPORT_2026-04-03.md`

### 10.3 联系方式

如有问题，请参考：
- 故障排查：`FEDERATION_TEST_EXECUTION_GUIDE_2026-04-03.md` 第五章
- 测试方案：`FEDERATION_INTEROP_TEST_PLAN_2026-04-03.md`
- 验证映射：`FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`
