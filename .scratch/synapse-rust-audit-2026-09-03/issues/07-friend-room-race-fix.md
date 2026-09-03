# 07: friend_room_service 添加 Redis SETNX 分布式锁防竞态（P1-4）

**What to build:** 在 `synapse-services/src/friend_room_service/mod.rs` 的 `create_friend_list_room` 函数（行 176-228）中，在缓存 miss → DB miss 之后调用 `create_room()` 之前，加 Redis SETNX 分布式锁保护竞态窗口。

**Blocked by:** None（无依赖，立即可开始）

**Status:** ✅ done（已通过 40 个 friend_room 集成测试，cargo build --locked ✅）

- [x] 构造锁 key：`format!("friend_room_lock:{}", friend_user_id)`
- [x] 尝试 SETNX 设置锁（TTL 5s）
- [x] 若获取成功：执行 create_room，然后 DEL 锁
- [x] 若获取失败（另一请求正在创建）：wait + retry 或直接返回 409 Conflict
- [x] 锁的获取/释放过程记录 debug 日志
- [x] 考虑 Redis 完全不可用时的降级路径（放行，创建 room 幂等保护兜底）
- [x] cargo build --locked 通过
- [ ] 写并发测试：模拟两个并发请求，验证只创建一次房间

## 验证

**cargo build（SQLX_OFFLINE=true）**：✅ synapse-cache + synapse-services 编译通过

**集成测试**：`cargo test --features "test-utils privacy-ext voice-extended voip-tracking beacons server-notifications" --test integration friend_room`
- ✅ **40/40** friend_room 测试全部通过（耗时 211.91s）

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-cache/src/lib.rs` | `RedisCacheManager::set_nx()` + `RedisCacheManager::delete_lock()` + `CacheManager::try_acquire_lock()` + `CacheManager::release_lock()` |
| `synapse-services/src/friend_room_service/mod.rs` | 重构 `create_friend_list_room()` — 加 SETNX 分布式锁，锁内双重检查 DB，复用 `release_lock` finally 语义 |