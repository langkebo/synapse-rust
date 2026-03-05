# Checklist

## 数据库 Schema 修复
- [x] users 表字段命名符合规范：`is_admin`, `is_deactivated`, `is_shadow_banned`, `is_guest`
- [x] access_tokens 表字段命名符合规范：`is_valid`, `revoked_ts`, `expires_ts` (可空)
- [x] refresh_tokens 表字段命名符合规范：`token_hash`, `expires_at`, `is_revoked`, `revoked_ts`
- [x] devices 表字段命名符合规范：`created_ts`, `first_seen_ts`, `last_seen_ts`
- [x] 所有时间戳字段使用 BIGINT 类型
- [x] 所有布尔字段使用 `is_` 前缀

## Rust 代码修复
- [x] User 结构体字段类型正确：`is_admin: bool`, `is_deactivated: bool`
- [x] AccessToken 结构体字段类型正确：`expires_ts: Option<i64>`
- [x] RefreshToken 结构体字段命名正确：`expires_at`, `is_revoked`
- [x] Device 结构体字段命名正确：`created_ts`, `first_seen_ts`
- [x] 所有 SQL 查询字段名与数据库匹配

## 配置文件优化
- [x] homeserver.yaml 和 homeserver.local.yaml 已合并或明确区分
- [x] 敏感配置使用环境变量覆盖
- [x] 配置文件格式统一

## Docker 镜像
- [x] Docker 构建缓存已清理
- [x] 旧镜像已删除
- [x] 新镜像构建成功

## 验证
- [x] cargo build --release 无错误
- [x] cargo test 测试通过 (1141 passed; 0 failed)
- [x] 数据库迁移可正常执行
