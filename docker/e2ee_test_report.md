# 端到端加密API测试报告

**测试时间**: 2026-02-05 09:41:10

## 测试结果汇总

| 序号 | API | 方法 | 状态 | 结果 |
|------|-----|------|------|------|
| 1 | `/_matrix/client/r0/keys/upload` | POST | ❌ CONN_ERR | 需要认证 |
| 2 | `/_matrix/client/r0/keys/query` | POST | ❌ CONN_ERR | 需要认证 |
| 3 | `/_matrix/client/r0/keys/claim` | POST | ❌ CONN_ERR | 需要认证 |
| 4 | `/_matrix/client/r0/keys/changes` | GET | ❌ CONN_ERR | 需要认证 |
| 5 | `/_matrix/client/r0/rooms/!hU1S_lh9PJl93a-zGJY1SUlX:cjystx.top/keys/distribution` | GET | ❌ CONN_ERR | 需要认证 |
| 6 | `/_matrix/client/r0/sendToDevice/m.room.encrypted/test_txn_123` | PUT | ❌ CONN_ERR | 需要认证 |

## 详细结果

### 上传密钥

- **端点**: `POST /_matrix/client/r0/keys/upload`
- **状态码**: CONN_ERR
- **响应**: {}
- **错误**: ('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))

### 查询密钥

- **端点**: `POST /_matrix/client/r0/keys/query`
- **状态码**: CONN_ERR
- **响应**: {}
- **错误**: ('Connection aborted.', ConnectionResetError(104, 'Connection reset by peer'))

### 声明密钥

- **端点**: `POST /_matrix/client/r0/keys/claim`
- **状态码**: CONN_ERR
- **响应**: {}
- **错误**: ('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))

### 密钥变更

- **端点**: `GET /_matrix/client/r0/keys/changes`
- **状态码**: CONN_ERR
- **响应**: {}
- **错误**: ('Connection aborted.', ConnectionResetError(104, 'Connection reset by peer'))

### 房间密钥分发

- **端点**: `GET /_matrix/client/r0/rooms/!hU1S_lh9PJl93a-zGJY1SUlX:cjystx.top/keys/distribution`
- **状态码**: CONN_ERR
- **响应**: {}
- **错误**: ('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))

### 发送设备消息

- **端点**: `PUT /_matrix/client/r0/sendToDevice/m.room.encrypted/test_txn_123`
- **状态码**: CONN_ERR
- **响应**: {}
- **错误**: ('Connection aborted.', ConnectionResetError(104, 'Connection reset by peer'))

