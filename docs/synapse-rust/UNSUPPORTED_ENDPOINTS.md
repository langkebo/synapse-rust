# synapse-rust 显式未支持接口清单

> 日期：2026-04-05  
> 口径：这些接口当前“存在路由”，但项目明确不支持对应能力；必须返回 Matrix 标准错误 `M_UNRECOGNIZED`，避免 200 空成功或返回假数据。

---

## 一、清单

| 能力 | 端点 | 当前行为 | 代码证据 | 测试证据 |
| --- | --- | --- | --- | --- |
| 客户端配置 | `GET /_matrix/client/v1/config/client` | `400` + `M_UNRECOGNIZED` | [assembly.rs](../../src/web/routes/assembly.rs) | [api_enhanced_features_tests.rs](../../tests/integration/api_enhanced_features_tests.rs) |
| 第三方桥接（Third-party） | `GET /_matrix/client/v3/thirdparty/protocols`<br>`GET /_matrix/client/v3/thirdparty/protocol/{protocol}`<br>`GET /_matrix/client/v3/thirdparty/location`<br>`GET /_matrix/client/v3/thirdparty/user`<br>`GET /_matrix/client/r0/thirdparty/protocols`<br>`GET /_matrix/client/r0/thirdparty/protocol/{protocol}` | `400` + `M_UNRECOGNIZED` | [thirdparty.rs](../../src/web/routes/thirdparty.rs) | [api_enhanced_features_tests.rs](../../tests/integration/api_enhanced_features_tests.rs) |
| 房间级举报 | `POST /_matrix/client/v3/rooms/{room_id}/report` | `400` + `M_UNRECOGNIZED`（在通过鉴权和房间存在性校验后） | [directory_reporting.rs](../../src/web/routes/directory_reporting.rs) | [api_placeholder_contract_p0_tests.rs](../../tests/integration/api_placeholder_contract_p0_tests.rs) |
| 房间初始同步（legacy） | `GET /_matrix/client/r0/rooms/{room_id}/initialSync` | `400` + `M_UNRECOGNIZED` | [room.rs](../../src/web/routes/handlers/room.rs) | [api_placeholder_contract_p0_tests.rs](../../tests/integration/api_placeholder_contract_p0_tests.rs) |
| 语音转写（ASR） | `POST /_matrix/client/v1/voice/transcription` | `400` + `M_UNRECOGNIZED` | [voice.rs](../../src/web/routes/voice.rs) | [voice_routes_tests.rs](../../tests/integration/voice_routes_tests.rs) |
| 联邦第三方邀请交换（3PID invite） | `PUT /_matrix/federation/v1/exchange_third_party_invite/{room_id}` | `400` + `M_UNRECOGNIZED`（在通过联邦签名鉴权后） | [federation.rs](../../src/web/routes/federation.rs) | [api_federation_signature_auth_tests.rs](../../tests/integration/api_federation_signature_auth_tests.rs) |

---

## 二、增量规则

1. 新增“显式未支持”的对外端点时，必须：
   - 在路由中返回 `M_UNRECOGNIZED`
   - 在集成测试中断言 `errcode == "M_UNRECOGNIZED"`
   - 同步更新本清单
2. 禁止在“未支持”端点返回 `200` 或固定假数据（除 Matrix 规范允许的空成功响应外）。
