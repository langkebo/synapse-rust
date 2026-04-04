# P2 任务评估与建议

> 日期：2026-04-03  
> 文档类型：任务评估  
> 说明：评估 P2 级别架构收口任务的可行性和优先级

## 一、P2 任务概览

### P2-1：拆分总容器职责
- **目标**：避免 `ServiceContainer` 继续膨胀
- **当前状态**：`src/services/container.rs` 716 行，86+ 字段
- **复杂度**：高

### P2-2：拆分总路由装配职责
- **目标**：减少总路由中心堆叠
- **当前状态**：`src/web/routes/assembly.rs` 302 行，122 个路由调用
- **复杂度**：高

### P2-3：建立文档索引与归档规则
- **目标**：让新成员能快速知道该读哪份文档
- **当前状态**：✅ 已完成（`PROJECT_REVIEW_INDEX_2026-04-03.md`）
- **复杂度**：低

### P2-4：建立性能与回滚基线
- **目标**：把性能目标变成可验证基线
- **当前状态**：未开始
- **复杂度**：中

## 二、P2-1 评估：拆分总容器职责

### 当前问题

`ServiceContainer` 包含 86+ 字段，涵盖：
- 存储层（UserStorage, DeviceStorage, RoomStorage 等）
- 服务层（AuthService, RoomService, SyncService 等）
- E2EE 服务（DeviceKeyService, MegolmService, CrossSigningService 等）
- Federation 服务（FriendFederation, EventAuthChain 等）
- 基础设施（Redis, Metrics, TaskQueue 等）

### 拆分方案建议

**方案 A：按能力域分组**

```rust
pub struct ServiceContainer {
    pub core: CoreServices,           // 用户、设备、认证
    pub room: RoomServices,            // 房间、成员、事件
    pub e2ee: E2EEServices,            // 端到端加密
    pub federation: FederationServices, // 联邦
    pub admin: AdminServices,          // 管理
    pub optional: OptionalServices,    // 可选能力
    pub infrastructure: Infrastructure, // 基础设施
}
```

**方案 B：按层次分组**

```rust
pub struct ServiceContainer {
    pub storage: StorageLayer,    // 所有存储
    pub services: ServiceLayer,   // 所有服务
    pub infrastructure: InfraLayer, // 基础设施
}
```

### 实施复杂度

- **高**：需要修改所有引用 `ServiceContainer` 的代码
- **影响范围**：整个代码库
- **风险**：可能引入回归问题
- **时间估算**：需要 2-3 天的设计 + 3-5 天的实施 + 2-3 天的测试

### 建议

**暂不实施**，原因：
1. 当前 `ServiceContainer` 虽然大，但功能正常
2. 重构风险高，可能引入新问题
3. 应该在验证证据补齐后再考虑
4. 需要专门的架构设计阶段

## 三、P2-2 评估：拆分总路由装配职责

### 当前问题

`src/web/routes/assembly.rs` 包含 122 个路由调用，混合了：
- 标准 Matrix Client-Server API
- Synapse Admin API
- 可选能力路由（OIDC, SAML, CAS）
- 扩展功能路由

### 拆分方案建议

**方案 A：按 API 类型分组**

```rust
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .merge(create_client_server_router(state.clone()))
        .merge(create_server_server_router(state.clone()))
        .merge(create_admin_router(state.clone()))
        .merge(create_optional_router(state.clone()))
}
```

**方案 B：按能力域分组**

```rust
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .merge(create_core_router(state.clone()))      // 用户、房间、同步
        .merge(create_e2ee_router(state.clone()))      // E2EE
        .merge(create_federation_router(state.clone())) // 联邦
        .merge(create_admin_router(state.clone()))     // 管理
        .merge(create_optional_router(state.clone()))  // 可选
}
```

### 实施复杂度

- **中高**：需要重新组织路由结构
- **影响范围**：路由层和测试
- **风险**：可能影响路由优先级和匹配顺序
- **时间估算**：需要 1-2 天的设计 + 2-3 天的实施 + 1-2 天的测试

### 建议

**可以考虑实施**，但需要：
1. 先完成验证证据补齐
2. 确保有完整的集成测试覆盖
3. 分阶段实施，每次只拆分一个模块

## 四、P2-4 评估：建立性能与回滚基线

### 目标

建立可验证的性能基线：
- 核心路径响应时间基准值
- 性能退化阈值
- 回滚指引

### 实施方案

1. **选择核心路径**
   - `/sync`
   - `/messages`
   - `/send`
   - `/login`
   - `/register`

2. **建立基准测试**
   - 使用 `criterion` 或类似工具
   - 记录 P50、P95、P99 延迟
   - 记录吞吐量

3. **定义退化阈值**
   - 例如：P95 延迟增加 > 20% 触发警告
   - 例如：吞吐量下降 > 30% 触发回滚

4. **创建回滚指引**
   - 性能退化的识别方法
   - 回滚决策流程
   - 回滚操作步骤

### 实施复杂度

- **中**：需要性能测试基础设施
- **影响范围**：CI 和监控
- **风险**：低
- **时间估算**：需要 2-3 天的设计 + 3-4 天的实施

### 建议

**可以考虑实施**，但优先级低于验证证据补齐。

## 五、总体建议

### 当前优先级排序

1. **P0/P1 任务**：✅ 已完成
   - 文档治理
   - 验证证据映射
   - 测试补充

2. **验证证据执行**：⏳ 进行中
   - 在 CI 中运行 AppService 集成测试
   - 执行 Federation 互操作测试
   - 调试本地测试环境

3. **P2-3**：✅ 已完成
   - 文档索引与归档规则

4. **P2-2**：⏸️ 可选
   - 拆分总路由装配职责
   - 建议在验证证据补齐后考虑

5. **P2-4**：⏸️ 可选
   - 建立性能与回滚基线
   - 建议在核心功能稳定后考虑

6. **P2-1**：⏸️ 暂不推荐
   - 拆分总容器职责
   - 风险高，收益不明确

### 下一步行动建议

**短期（本周）**：
1. 在 CI 环境中验证 AppService 集成测试
2. 执行 Federation 互操作测试
3. 根据测试结果更新能力基线

**中期（下周）**：
1. 如果测试通过，考虑升级 Federation 和 AppService 能力状态
2. 如果有时间，可以开始 P2-2（路由拆分）的设计

**长期（下月）**：
1. 考虑 P2-4（性能基线）
2. 评估 P2-1（容器拆分）的必要性

## 六、结论

**P2 任务中**：
- ✅ P2-3 已完成
- ⏸️ P2-1 暂不推荐（风险高，收益不明确）
- ⏸️ P2-2 可选（建议在验证证据补齐后考虑）
- ⏸️ P2-4 可选（建议在核心功能稳定后考虑）

**当前最重要的工作**：
1. 执行已创建的测试（AppService、Federation）
2. 根据测试结果更新能力状态
3. 确保核心功能的验证证据充分

**架构重构的时机**：
- 应该在验证证据充分、核心功能稳定后再考虑
- 需要专门的设计阶段，不适合在当前会话中完成
- 应该有明确的问题和收益分析

## 七、如果要推进 P2 任务

### P2-2（路由拆分）最小实施方案

如果决定推进 P2-2，建议采用最小化方案：

1. **第一阶段**：提取可选能力路由
   - 将 OIDC、SAML、CAS 路由提取到独立函数
   - 保持其他路由不变

2. **第二阶段**：提取 Admin 路由
   - 将所有 `/_synapse/admin` 路由提取到独立函数
   - 保持其他路由不变

3. **第三阶段**：按需继续拆分
   - 根据实际需要决定是否继续拆分

### P2-4（性能基线）最小实施方案

如果决定推进 P2-4，建议采用最小化方案：

1. **第一阶段**：选择 3 个核心路径
   - `/sync`
   - `/messages`
   - `/send`

2. **第二阶段**：建立基准测试
   - 使用 `criterion` 创建基准测试
   - 记录当前性能数据

3. **第三阶段**：定义阈值和监控
   - 定义性能退化阈值
   - 在 CI 中添加性能回归检测

## 八、参考资料

- 当前容器实现：`src/services/container.rs`
- 当前路由装配：`src/web/routes/assembly.rs`
- 项目整改清单：`PROJECT_REVIEW_ACTION_BACKLOG_2026-04-03.md`
- 优化执行总结：`OPTIMIZATION_SUMMARY_2026-04-03.md`
