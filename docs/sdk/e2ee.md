# 端到端加密 (E2EE) API

## 概述

端到端加密 (E2EE) 确保只有通信双方能够读取消息内容，服务器无法解密消息。

### 核心概念

- **Device Keys**: 每个设备的身份密钥
- **One-Time Keys**: 一次性密钥，用于 Olm 会话
- **Megolm**: 群组消息加密协议
- **Cross-Signing**: 交叉签名，用于身份验证
- **Key Backup**: 密钥备份，防止数据丢失

---

## 目录

- [上传设备密钥](#上传设备密钥)
- [查询设备密钥](#查询设备密钥)
- [声明一次性密钥](#声明一次性密钥)
- [密钥变更通知](#密钥变更通知)
- [设备到设备消息](#设备到设备消息)

---

## 上传设备密钥

### 上传设备密钥

**端点:** `POST /_matrix/client/r0/keys/upload`

**需要认证:** 是

**请求体:**
```typescript
interface KeysUploadRequest {
  device_keys: DeviceKeys;
  one_time_keys?: Record<string, OneTimeKey>;
}

interface DeviceKeys {
  user_id: string;
  device_id: string;
  algorithms: string[];
  keys: Record<string, string>;
  signatures: Record<string, Record<string, string>>;
  unsigned?: Record<string, any>;
}

interface OneTimeKey {
  key: string;          // Curve25519 密钥，Base64 编码
  fallback_key?: string;  // Ed25519 密钥，Base64 编码
}
```

**请求示例:**
```typescript
const uploadDeviceKeys = async (
  deviceId: string,
  publicKey: string,
  signature: Record<string, Record<string, string>>,
  accessToken: string
) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/upload`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      device_keys: {
        user_id: getCurrentUserId(),
        device_id: deviceId,
        algorithms: ['m.olm.v1.curve25519-aes-sha2'],
        keys: {
          'curve25519:': publicKey
        },
        signatures: signature
      }
    })
  });
  return handleApiResponse<{
    one_time_key_counts: Record<string, number>;
  }>(response);
};

// 上传一次性密钥
const uploadOneTimeKeys = async (
  keys: Record<string, string>,
  accessToken: string
) => {
  const oneTimeKeys: Record<string, OneTimeKey> = {};

  for (const [keyId, key] of Object.entries(keys)) {
    oneTimeKeys[keyId] = { key };
  }

  const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/upload`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      one_time_keys: oneTimeKeys
    })
  });
  return handleApiResponse(response);
};
```

---

## 查询设备密钥

### 查询设备密钥

**端点:** `POST /_matrix/client/r0/keys/query`

**需要认证:** 是

**请求体:**
```typescript
interface KeysQueryRequest {
  device_keys: Record<string, string[]>;
  timeout?: number;
  token?: string;
}
```

**请求示例:**
```typescript
const queryKeys = async (
  userIds: string[],
  accessToken: string
) => {
  const deviceKeys: Record<string, string[]> = {};

  for (const userId of userIds) {
    deviceKeys[userId] = ['*'];  // 查询所有设备
  }

  const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/query`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      device_keys: deviceKeys,
      timeout: 10000
    })
  });
  return handleApiResponse<{
    device_keys: Record<string, DeviceKeysInfo>;
    failures: Record<string, any>;
  }>(response);
};

interface DeviceKeysInfo {
  user_id: string;
  device_id: string;
  algorithms: string[];
  keys: Record<string, string>;
  signatures: Record<string, Record<string, string>>;
  unsigned?: Record<string, any>;
}
```

---

## 声明一次性密钥

### 声明一次性密钥

**端点:** `POST /_matrix/client/r0/keys/claim`

**需要认证:** 是

**请求体:**
```typescript
interface KeysClaimRequest {
  one_time_keys: Record<string, string>;
  timeout?: number;
}
```

**请求示例:**
```typescript
const claimKeys = async (
  userId: string,
  deviceId: string,
  accessToken: string
) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/claim`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      one_time_keys: {
        [`${userId}:${deviceId}`]: 'signed_curve25519'  // 密钥算法
      },
      timeout: 10000
    })
  });
  return handleApiResponse<{
    one_time_keys: Record<string, OneTimeKey>;
    failures: Record<string, any>;
  }>(response);
};
```

---

## 密钥变更通知

### 获取密钥变更通知

**端点:** `GET /_matrix/client/r0/keys/changes`

**需要认证:** 是

**参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| from | string | 否 | 起始 token |
| to | string | 否 | 结束 token |

**请求示例:**
```typescript
const getKeyChanges = async (from?: string, accessToken: string) => {
  const url = new URL(`${BASE_URL}/_matrix/client/r0/keys/changes`);
  if (from) url.searchParams.set('from', from);

  const response = await fetch(url.toString(), {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    changed: Record<string, DeviceKeyInfo[]>;
    left: string[];
  }>(response);
};
```

---

## 设备到设备消息

### 发送设备到设备消息

**端点:** `PUT /_matrix/client/r0/sendToDevice/{event_type}/{txn_id}`

**需要认证:** 是

**请求体:**
```typescript
interface SendToDeviceRequest {
  messages: Record<string, ToDeviceMessage>;
}

interface ToDeviceMessage {
  [userId: string]: {
    'm.room.encrypted'?: EncryptedContent;
    'm.room.message'?: any;
    'm.room_key_request'?: any;
    // 其他自定义事件类型
  };
}
```

**请求示例:**
```typescript
const sendToDevice = async (
  userId: string,
  eventType: string,
  content: any,
  accessToken: string
) => {
  const txnId = Date.now().toString();

  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/sendToDevice/${encodeURIComponent(eventType)}/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        messages: {
          [userId]: {
            [eventType]: content
          }
        }
      })
    }
  );
  return handleApiResponse(response);
};

// 发送房间密钥请求
const requestRoomKey = async (
  roomId: string,
  userIds: string[],
  deviceId: string,
  accessToken: string
) => {
  const txnId = Date.now().toString();

  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/sendToDevice/m.room_key_request/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        messages: userIds.reduce((acc, userId) => {
          acc[userId] = {
            'action': 'request',
            'room_id': roomId,
            'request_id': txnId,
            'requesting_device_id': deviceId
          };
          return acc;
        }, {} as Record<string, any>)
      })
    }
  );
  return handleApiResponse(response);
};
```

---

## 完整 E2EE 服务示例

```typescript
class E2EEService {
  constructor(private auth: AuthService) {}

  // 上传设备密钥
  async uploadDeviceKeys(keys: DeviceKeys) {
    const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/upload`, {
      method: 'POST',
      headers: this.auth.getAuthHeaders(),
      body: JSON.stringify({ device_keys: keys })
    });
    return this.auth.handleResponse(response);
  }

  // 查询用户设备密钥
  async queryUserKeys(userIds: string[]) {
    const deviceKeys: Record<string, string[]> = {};
    for (const userId of userIds) {
      deviceKeys[ userId] = ['*'];
    }

    const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/query`, {
      method: 'POST',
      headers: this.auth.getAuthHeaders(),
      body: JSON.stringify({ device_keys: deviceKeys })
    });
    return this.auth.handleResponse(response);
  }

  // 声明一次性密钥
  async claimOneTimeKeys(targets: Array<{userId: string, deviceId: string}>) {
    const oneTimeKeys: Record<string, string> = {};

    for (const {userId, deviceId} of targets) {
      oneTimeKeys[`${userId}:${deviceId}`] = 'signed_curve25519';
    }

    const response = await fetch(`${BASE_URL}/_matrix/client/r0/keys/claim`, {
      method: 'POST',
      headers: this.auth.getAuthHeaders(),
      body: JSON.stringify({
        one_time_keys: oneTimeKeys,
        timeout: 10000
      })
    });
    return this.auth.handleResponse(response);
  }

  // 发送设备到设备消息
  async sendToDevice(
    userId: string,
    eventType: string,
    content: any
  ) {
    const txnId = Date.now().toString();

    const response = await fetch(
      `${BASE_URL}/_matrix/client/r0/sendToDevice/${encodeURIComponent(eventType)}/${txnId}`,
      {
        method: 'PUT',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify({
          messages: {
            [userId]: {
              [eventType]: content
            }
          }
        })
      }
    );
    return this.auth.handleResponse(response);
  }

  // 获取密钥变更
  async getKeyChanges(from?: string) {
    const url = new URL(`${BASE_URL}/_matrix/client/r0/keys/changes`);
    if (from) url.searchParams.set('from', from);

    const response = await fetch(url.toString(), {
      headers: this.auth.getAuthHeaders()
    });
    return this.auth.handleResponse(response);
  }
}
```

---

## E2EE 加密流程

### 1:1 聊天加密 (Olm)

```typescript
// 使用 Olm 库进行端到端加密
import * as Olm from '@matrix/org.olm';

class OlmEncryption {
  private account: Olm.Account;
  private session: Map<string, Olm.Session> = new Map();

  async createAccount() {
    this.account = Olm.Account.create();
    this.account.generate_one_time_keys(1);

    const identityKeys = JSON.parse(this.account.identity_keys());
    const oneTimeKeys = JSON.parse(this.account.one_time_keys());

    // 上传到服务器
    await this.uploadKeys(identityKeys, oneTimeKeys);
  }

  async encryptMessage(userId: string, message: string): Promise<string> {
    let session = this.session.get(userId);

    if (!session) {
      // 创建新会话
      session = Olm.Session.create();
      this.session.set(userId, session);

      // 获取对方的公钥并预共享
      await this.claimOneTimeKey(userId);
    }

    const encrypted = session.encrypt(message);
    return encrypted;
  }

  async decryptMessage(userId: string, encryptedMessage: string): Promise<string> {
    const session = this.session.get(userId);
    if (!session) {
      throw new Error('No session found for user');
    }

    return session.decrypt(encryptedMessage);
  }

  private async uploadKeys(identityKeys: any, oneTimeKeys: any) {
    // 实现上传逻辑
  }

  private async claimOneTimeKey(userId: string) {
    // 实现密钥声明逻辑
  }
}
```

### 群组聊天加密 (Megolm)

```typescript
class MegolmEncryption {
  private outboundSessions: Map<string, any> = new Map();
  private inboundSessions: Map<string, any> = new Map();

  async encryptGroupMessage(roomId: string, userIds: string[], message: string) {
    // 获取或创建外出会话
    let session = this.outboundSessions.get(roomId);

    if (!session) {
      session = await this.createOutboundSession(roomId, userIds);
      this.outboundSessions.set(roomId, session);
    }

    return session.encrypt(message);
  }

  async decryptGroupMessage(roomId: string, senderId: string, message: string) {
    const session = this.inboundSessions.get(`${roomId}:${senderId}`);
    if (!session) {
      throw new Error('No inbound session found');
    }

    return session.decrypt(message);
  }

  private async createOutboundSession(roomId: string, userIds: string[]) {
    // 实现 Megolm 会话创建
    return {};
  }
}
```
