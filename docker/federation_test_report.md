# 联邦通信API测试报告

**测试时间**: 2026-02-05 09:33:22

## 测试结果汇总

| 序号 | API | 方法 | 状态 | 结果 |
|------|-----|------|------|------|
| 1 | `/_matrix/federation/v2/server` | GET | ✅ 200 |  |
| 2 | `/_matrix/key/v2/server` | GET | ✅ 200 |  |
| 3 | `/_matrix/federation/v2/query/cjystx.top/ed25519:auto` | GET | ✅ 200 |  |
| 4 | `/_matrix/key/v2/query/cjystx.top/ed25519:auto` | GET | ✅ 200 |  |
| 5 | `/_matrix/federation/v1/version` | GET | ✅ 200 |  |
| 6 | `/_matrix/federation/v1` | GET | ✅ 200 |  |
| 7 | `/_matrix/federation/v1/publicRooms` | GET | ✅ 200 |  |
| 8 | `/_matrix/federation/v1/send/test_txn` | PUT | ✅ 401 | 需要联邦签名 |
| 9 | `/_matrix/federation/v1/make_join/!room:test/@user:test` | GET | ✅ 401 | 需要联邦签名 |
| 10 | `/_matrix/federation/v1/make_leave/!room:test/@user:test` | GET | ✅ 401 | 需要联邦签名 |
| 11 | `/_matrix/federation/v1/send_join/!room:test/$event` | PUT | ✅ 401 | 需要联邦签名 |
| 12 | `/_matrix/federation/v1/send_leave/!room:test/$event` | PUT | ✅ 401 | 需要联邦签名 |
| 13 | `/_matrix/federation/v1/invite/!room:test/$event` | PUT | ✅ 401 | 需要联邦签名 |
| 14 | `/_matrix/federation/v1/get_missing_events/!room:test` | POST | ✅ 401 | 需要联邦签名 |
| 15 | `/_matrix/federation/v1/get_event_auth/!room:test/$event` | GET | ✅ 401 | 需要联邦签名 |
| 16 | `/_matrix/federation/v1/state/!room:test` | GET | ✅ 401 | 需要联邦签名 |
| 17 | `/_matrix/federation/v1/event/$event` | GET | ✅ 401 | 需要联邦签名 |
| 18 | `/_matrix/federation/v1/state_ids/!room:test` | GET | ✅ 401 | 需要联邦签名 |
| 19 | `/_matrix/federation/v1/query/directory/room/!room:test` | GET | ✅ 401 | 需要联邦签名 |
| 20 | `/_matrix/federation/v1/query/profile/@user:test` | GET | ✅ 401 | 需要联邦签名 |
| 21 | `/_matrix/federation/v1/backfill/!room:test` | GET | ✅ 401 | 需要联邦签名 |
| 22 | `/_matrix/federation/v1/keys/claim` | POST | ✅ 401 | 需要联邦签名 |
| 23 | `/_matrix/federation/v1/keys/upload` | POST | ✅ 401 | 需要联邦签名 |
| 24 | `/_matrix/federation/v2/key/clone` | POST | ✅ 401 | 需要联邦签名 |
| 25 | `/_matrix/federation/v2/user/keys/query` | POST | ✅ 401 | 需要联邦签名 |
| 26 | `/_matrix/federation/v1/keys/query` | POST | ✅ 405 | 需要联邦签名 |
| 27 | `/_matrix/federation/v1/version` | GET | ✅ 200 | 公开端点 |
| 28 | `/_matrix/key/v2/server` | GET | ✅ 200 | 公开端点 |
| 29 | `/_matrix/federation/v1/members/!room:test` | GET | ✅ 200 | 需要联邦签名 |
| 30 | `/_matrix/federation/v1/members/!room:test/joined` | GET | ✅ 200 | 需要联邦签名 |
| 31 | `/_matrix/federation/v1/user/devices/@test` | GET | ✅ 200 | 需要联邦签名 |
| 32 | `/_matrix/federation/v1/room_auth/!room:test` | GET | ✅ 200 | 需要联邦签名 |

## 详细结果

