-- MSC4108 QR 登录 token 持久化（审查 #3）
--
-- 已登录设备生成短时 login token（60s TTL），新设备经 MSC4108 安全通道
-- 接收后以 m.login.token 兑换 access token。token 单次使用，消费即删除
-- （DELETE ... RETURNING，防重放），跨 worker/重启不丢。

CREATE TABLE IF NOT EXISTS login_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_login_tokens_expires
ON login_tokens(expires_at);
