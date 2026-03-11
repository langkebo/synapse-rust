# API 扩展测试报告

> 测试时间: 2026年 3月10日 星期二 21时56分54秒 CST
> 测试环境: https://matrix.cjystx.top
> 用户: @ext_test_1773151013:cjystx.top

---

## 测试统计

| 指标 | 数值 |
|------|------|
| 总测试数 | 17 |
| 通过数 | 11 |
| 失败数 | 6 |
| 通过率 | 64% |

---

## 测试详情

| 状态 | 类别 | 端点 | 备注 |
|------|------|------|------|
| ✅ | ACCOUNT | GET /_matrix/client/v3/user/@ext_test_1773151013:cjystx.top/account_data/m.direct | - |
| ✅ | ACCOUNT | PUT /_matrix/client/v3/user/@ext_test_1773151013:cjystx.top/account_data/m.custom | - |
| ✅ | ROOM | GET /_matrix/client/v3/rooms/!q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top/state | - |
| ✅ | ROOM | GET /_matrix/client/v3/rooms/!q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top/members | - |
| ✅ | ROOM | GET /_matrix/client/v3/rooms/!q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top/messages?limit=10 | - |
| ✅ | FILTER | GET /_matrix/client/v3/user/@ext_test_1773151013:cjystx.top/filter/Odo4PIQcIj4sRjqt | - |
| ✅ | RECEIPT | POST /_matrix/client/v3/rooms/!q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top/receipt/m.read/$1773151014223$zz0rIl7kUyuGeav18ldD8eTm:cjystx.top | - |
| ✅ | RECEIPT | POST /_matrix/client/v3/rooms/!q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top/read_markers | - |
| ✅ | TYPING | PUT /_matrix/client/v3/rooms/!q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top/typing/@ext_test_1773151013:cjystx.top | - |
| ❌ | ADMIN | GET /_synapse/admin/v1/status | HTTP 403: {"errcode":"M_FORBIDDEN","error":"Admin  |
| ❌ | ADMIN | GET /_synapse/admin/v1/config | HTTP 403: {"errcode":"M_FORBIDDEN","error":"Admin  |
| ❌ | ADMIN | GET /_synapse/admin/v1/server_stats | HTTP 403: {"errcode":"M_FORBIDDEN","error":"Admin  |
| ❌ | ADMIN | GET /_synapse/admin/v1/statistics | HTTP 403: {"errcode":"M_FORBIDDEN","error":"Admin  |
| ❌ | ADMIN | GET /_synapse/admin/v1/user_stats | HTTP 403: {"errcode":"M_FORBIDDEN","error":"Admin  |
| ❌ | ADMIN | GET /_synapse/admin/v1/media_stats | HTTP 403: {"errcode":"M_FORBIDDEN","error":"Admin  |
| ✅ | DEVICE | GET /_matrix/client/v3/devices/ciFaZAveZaX32gxabavnHA | - |
| ✅ | DEVICE | PUT /_matrix/client/v3/devices/ciFaZAveZaX32gxabavnHA | - |

---

## 测试环境信息

- 服务器: https://matrix.cjystx.top
- 用户ID: @ext_test_1773151013:cjystx.top
- 设备ID: ciFaZAveZaX32gxabavnHA
- 房间ID: !q1Sm2Hk_FTK4spBvDg0JyB48:cjystx.top

---

*报告生成时间: 2026年 3月10日 星期二 21时56分54秒 CST*
