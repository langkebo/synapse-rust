# synapse-rust API 优化方案汇总

> 版本: 1.0
> 日期: 2026-03-27
> 目标: 消除功能重复，保持向后兼容

---

## 一、优化模块概览

| 模块 | 优化前路由 | 优化后路由 | 减少 |
|------|-----------|-----------|------|
| **friend_room** | 43 | 15 | 65% |
| **account_data** | 12 | 6 | 50% |
| **media** | 21 | 14 | 33% |
| **device** | 8 | 4 | 50% |
| **e2ee_routes** | 26 | 17 | 35% |
| **room_summary** | 23 | 16 | 30% |
| **search** | 13 | 9 | 31% |
| **总计** | **146** | **81** | **44%** |

---

## 二、核心优化策略

### 2.1 版本统一

```
使用 {version} 路径参数统一多个版本:
  /_matrix/client/v3/...    → /_matrix/client/{version}/...
  /_matrix/client/r0/...   → /
  /_matrix/client/v1/...   → /
```

### 2.2 功能合并

| 功能 | 涉及模块 | 解决方案 |
|------|---------|---------|
| threads | search + thread | 统一使用 thread 模块 |
| sync | room_summary + sync | 保留 sync 模块 |
| unread/clear | room_summary + sync | 使用 sync 已读 API |

---

## 三、详细方案文件

| 文件 | 内容 |
|------|------|
| `friend_room-optimization.md` | 好友模块优化方案 |
| `account_data-optimization.md` | 账户数据模块优化方案 |
| `media-optimization.md` | 媒体模块优化方案 |
| `device-optimization.md` | 设备模块优化方案 |
| `e2ee-optimization.md` | 端到端加密模块优化方案 |
| `room_summary-optimization.md` | 房间摘要模块优化方案 |
| `search-optimization.md` | 搜索模块优化方案 |

---

## 四、实施优先级

### 4.1 第一阶段 (低风险)

1. **添加版本重定向中间件** - 统一路由处理
2. **friend_room** - 合并 43→15 路由
3. **account_data** - 合并 12→6 路由
4. **device** - 合并 8→4 路由

### 4.2 第二阶段 (中风险)

5. **e2ee_routes** - 合并 26→17 路由
6. **media** - 合并 21→14 路由
7. **search** - 合并 13→9 路由

### 4.3 第三阶段 (高风险)

8. **room_summary** - 删除重复功能
   - `/summary/sync` → 使用 `/sync`
   - `/summary/unread/clear` → 使用已读 API

---

## 五、向后兼容配置

```toml
[features]
default = ["compat-r0", "compat-v1"]
compat-r0 = []      # 启用 r0 兼容
compat-v1 = []      # 启用 v1 兼容
```

```yaml
# config.yaml
api:
  version_compat:
    enable_r0: true
    enable_v1: true
    enable_media_r1: true
```

---

## 六、废弃端点警告

在日志中添加版本废弃警告:

```rust
if version == "r0" || version == "v1" {
    tracing::warn!(
        target: "api_deprecation",
        "Client {} using deprecated API version {}",
        user_id,
        version
    );
}
```

---

## 七、风险评估

| 模块 | 风险 | 影响 | 缓解措施 |
|------|------|------|----------|
| friend_room | 低 | 旧客户端 | 充分测试 |
| account_data | 低 | 账户数据 | 兼容模式 |
| media | 中 | 媒体上传 | 保留 v1 配额 |
| device | 低 | 设备列表 | 无影响 |
| e2ee | 中 | 加密功能 | 保持逻辑 |
| room_summary | 中 | 房间摘要 | 保留别名 |
| search | 中 | 搜索功能 | 添加警告 |

---

## 八、测试计划

### 8.1 回归测试
- 所有现有端点正常工作
- 版本重定向正确

### 8.2 兼容性测试
- Matrix 官方 SDK
- Element 客户端

### 8.3 性能测试
- 路由匹配性能
- 并发处理能力

---

## 九、预期收益

| 指标 | 优化前 | 优化后 | 变化 |
|------|--------|--------|------|
| 总路由数 | 656 | ~400 | -39% |
| 重复路由 | ~60 | 0 | -100% |
| 代码重复 | 30% | ~5% | -83% |
| 维护成本 | 高 | 中 | -50% |

---

## 十、下一步

1. 审阅各模块详细方案
2. 确认实施优先级
3. 开始第一阶段实现
4. 完整测试验证