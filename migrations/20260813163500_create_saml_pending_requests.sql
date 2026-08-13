-- SAML AuthnRequest 待处理记录（SSO 会话跨 worker 一致，审查 #3）
--
-- SSO redirect 阶段以 relay_state 为 key 记录 request_id 与过期时间，
-- callback 阶段原子消费（DELETE ... RETURNING）并校验 InResponseTo，
-- 防重放，且跨实例/重启不丢会话。

CREATE TABLE IF NOT EXISTS saml_pending_requests (
    id BIGSERIAL PRIMARY KEY,
    relay_state TEXT NOT NULL UNIQUE,
    request_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_saml_pending_expires
ON saml_pending_requests(expires_at);
