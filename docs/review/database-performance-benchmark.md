# 数据库性能基准对比（第二轮修复）

## 目标
- 验证第二轮数据库修复后关键查询无显著回归
- 重点覆盖房间摘要、注册令牌、后台更新相关路径

## 基准查询清单
- `GET /_matrix/client/v3/rooms/{room_id}/summary/members`
  - 关注：`room_summary_members` 按 `last_active_ts` 排序
- `GET /_matrix/client/v3/rooms/{room_id}/summary/stats`
  - 关注：`room_summary_stats.total_media` 读取
- `POST /_synapse/admin/v1/registration_tokens`
  - 关注：`registration_tokens.token_type` 写入与索引命中
- `POST /_synapse/admin/v1/background_updates/cleanup_locks`
  - 关注：`background_update_locks` 清理路径

## 建议采集指标
- 延迟：P50 / P95 / P99
- 资源：平均扫描行数、缓冲命中率、CPU 时间
- 执行计划：是否走预期索引、是否出现 Seq Scan 热点

## 对比方法
- 基线组：应用第二轮迁移前
- 修复组：应用 `20260304000001` 与 `20260304000002` 后
- 工具建议：
  - `EXPLAIN (ANALYZE, BUFFERS)` 采集 SQL 执行细节
  - API 压测脚本固定并发、固定数据集、固定预热轮次

## 验收阈值
- 关键接口 P95 不高于基线 +10%
- 不允许出现“缺列/缺表”导致的 5xx
- 关键查询必须可稳定返回且类型映射正确

## 当前状态
- 代码级验证：`cargo check` 已通过
- 运行级基准：待在开发环境执行迁移后采集
