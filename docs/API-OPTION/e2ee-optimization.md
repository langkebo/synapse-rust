# E2EE（端到端加密）模块优化方案

## 一、当前实现判断

`src/web/routes/e2ee_routes.rs` 的核心特点是：

- `r0` 与 `v3` 在 `keys/upload`、`keys/query`、`keys/claim`、`keys/changes`、`sendToDevice`、`rooms/{room_id}/keys/distribution` 等端点上大量复用同一处理函数
- `v3` 额外提供了设备验证、信任和安全备份等扩展能力
- 这类模块很适合做**子路由复用**，但不需要改成 `{version}` 参数路由

---

## 二、与 Matrix 规范对齐后的结论

对 E2EE 来说，最稳妥的方案是：

1. 保留 `/_matrix/client/r0/*` 和 `/_matrix/client/v3/*` 公开路径
2. 通过内部共享子 `Router` 复用同构端点
3. 让 v3 独有安全能力继续单独挂载

不建议：

- 给每个 handler 新增 `Path(version): Path<String>`
- 用 HTTP 重定向把 r0 指到 v3
- 在文档中假设已有 `compat-r0-e2ee` feature

---

## 三、推荐重构方式

### 3.1 提取公共 keys 子路由

```rust
pub fn create_e2ee_router(state: AppState) -> Router<AppState> {
    let compat_router = Router::new()
        .route("/keys/upload", post(upload_keys))
        .route("/keys/query", post(query_keys))
        .route("/keys/claim", post(claim_keys))
        .route("/keys/changes", get(key_changes))
        .route("/keys/signatures/upload", post(upload_signatures))
        .route(
            "/rooms/{room_id}/keys/distribution",
            get(room_key_distribution),
        )
        .route(
            "/sendToDevice/{event_type}/{transaction_id}",
            put(send_to_device),
        );

    let v3_only_router = Router::new()
        .route(
            "/_matrix/client/v3/device_verification/request",
            post(request_device_verification),
        )
        .route(
            "/_matrix/client/v3/device_verification/respond",
            post(respond_device_verification),
        );

    Router::new()
        .nest("/_matrix/client/r0", compat_router.clone())
        .nest("/_matrix/client/v3", compat_router)
        .merge(v3_only_router)
        .with_state(state)
}
```

### 3.2 如果存在少量版本差异

若某些端点未来在 `r0` 与 `v3` 响应字段上出现差异，也建议：

- 继续复用 service 层逻辑
- 在最外层 handler 做轻量分支
- 不要一开始就把所有版本信息塞入路径参数

---

## 四、模块拆分建议

| 子域 | 当前情况 | 推荐动作 |
|------|----------|----------|
| `keys/*` | r0 / v3 大量同构 | 抽公共子路由 |
| `sendToDevice` | r0 / v3 同构 | 抽公共子路由 |
| `keys/signatures/upload` | 已复用 | 并入公共子路由 |
| `device_verification/*` | 仅 v3 | 保持独立 |
| `device_trust/*` | 仅 v3 | 保持独立 |
| `security/summary` | 仅 v3 | 保持独立 |
| `keys/backup/secure/*` | 仅 v3 | 保持独立 |

---

## 五、可落地收益

### 5.1 真实收益

- 降低重复 `.route()` 注册数量
- 让 `r0` / `v3` 同构端点集中管理
- 减少后续维护时“改了 v3 忘了 r0”的风险

### 5.2 不夸大的收益

这里更适合强调**维护性提升**，而不是给出过于精确的“总路由数下降百分比”。
因为当前文件中既有完全同构端点，也有 v3 独有扩展，直接按简单数量相减容易误导。

---

## 六、向后兼容建议

| 项目 | 建议 |
|------|------|
| r0 保留 | 是 |
| v3 保留 | 是 |
| v1 支持 | 当前文档不假设存在 |
| 弃用方式 | 若后续需要，再通过运行时配置和日志统计推进 |

如果未来要做兼容治理，建议写成“可新增配置项”，而不是假设 Cargo feature 已存在。

---

## 七、实施优先级

1. 抽取 `keys` 公共子路由
2. 抽取 `sendToDevice` 与房间密钥分发子路由
3. 保持 v3 扩展能力独立
4. 最后再考虑是否补访问统计或弃用日志

---

## 八、最终结论

E2EE 模块是当前代码库里**最适合用 `nest()` 做版本复用**的一类模块，但最佳方案是：

1. **保留 r0 与 v3 的公开路径**
2. **复用内部子路由**
3. **不改 handler 签名去接收通用 `version`**
4. **不把兼容机制写成当前已经存在的 feature flag**
