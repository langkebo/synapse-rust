# 消息与事件 API

## 目录

- [发送消息](#发送消息)
- [获取消息历史](#获取消息历史)
- [消息回执](#消息回执)
- [编辑/撤回消息](#编辑撤回消息)
- [打字状态](#打字状态)
- [自定义事件](#自定义事件)

---

## 发送消息

### 发送文本消息

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/send/m.room.message/{txn_id}`

**需要认证:** 是

**参数说明:**
- `room_id`: 房间 ID
- `txn_id`: 事务 ID (使用时间戳或随机唯一值)

**请求体:**
```typescript
interface TextMessage {
  msgtype: 'm.text';
  body: string;
  format?: string;           // 可选，格式化 HTML
  'm.relates_to'?: {
    rel_type: string;
    event_id: string;
  };
}
```

**请求示例:**
```typescript
const sendTextMessage = async (roomId: string, text: string, accessToken: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        msgtype: 'm.text',
        body: text
      })
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};

// 带引用的回复消息
const replyToMessage = async (roomId: string, text: string, eventId: string, accessToken: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        msgtype: 'm.text',
        body: text,
        'm.relates_to': {
          rel_type: 'm.in_reply_to',
          event_id: eventId
        }
      })
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};
```

**响应:**
```json
{
  "status": "ok",
  "data": {
    "event_id": "$event_id:server.com"
  }
}
```

---

### 发送其他类型消息

#### 图片消息
```typescript
const sendImageMessage = async (roomId: string, contentUrl: string, accessToken: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        msgtype: 'm.image',
        body: contentUrl,
        url: contentUrl,
        info: {
          mimetype: 'image/jpeg',
          w: 1024,
          h: 768,
          size: 123456
        }
      })
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};
```

#### 文件消息
```typescript
const sendFileMessage = async (roomId: string, contentUrl: string, filename: string, mimeType: string, accessToken: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        msgtype: 'm.file',
        body: contentUrl,
        url: contentUrl,
        filename,
        info: {
          mimetype: mimeType,
          size: 0
        }
      })
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};
```

#### 语音消息
```typescript
const sendVoiceMessage = async (roomId: string, audioUrl: string, duration: number, accessToken: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        msgtype: 'm.audio',
        body: audioUrl,
        url: audioUrl,
        info: {
          mimetype: 'audio/ogg',
          duration: duration
        }
      })
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};
```

---

## 获取消息历史

### 获取房间消息

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/messages`

**需要认证:** 是

**查询参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| from | string | 否 | 从哪个事件开始 |
| to | string | 否 | 到哪个事件结束 |
| dir | 'f' | 否 | 方向 ('f' 向前，'b' 向后) |
| limit | number | 否 | 限制数量 (默认 10) |
| filter | string | 否 | 过滤懒加载成员 |

**请求示例:**
```typescript
const getMessages = async (roomId: string, options: {
  from?: string;
  limit?: number;
  dir?: 'f' | 'b';
}, accessToken: string) => {
  const url = new URL(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/messages`);

  if (options.from) url.searchParams.set('from', options.from);
  if (options.limit) url.searchParams.set('limit', options.limit.toString());
  if (options.dir) url.searchParams.set('dir', options.dir);

  const response = await fetch(url.toString(), {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    chunk: Event[];
    start: string;
    end: string;
  }>(response);
};

interface Event {
  event_id: string;
  room_id: string;
  sender: string;
  type: string;
  origin_server_ts: number;
  content: any;
  prev_content?: any;
  state_key?: string;
}
```

---

## 消息回执

### 发送已读回执

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/receipt/m.read/{event_id}`

**需要认证:** 是

**请求示例:**
```typescript
const sendReadReceipt = async (roomId: string, eventId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/receipt/m.read/${eventId}`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({})
    }
  );
  return handleApiResponse(response);
};
```

### 设置已读标记

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/read_markers`

**请求体:**
```typescript
interface ReadMarkers {
  'm.read'?: string;           // 已读事件 ID
  'm.fully_read'?: string;     // 完全已读事件 ID
  'm.hidden'?: string;         // 隐藏消息事件 ID
}
```

**请求示例:**
```typescript
const setReadMarkers = async (roomId: string, readEventId: string, fullyReadEventId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/read_markers`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        'm.read': readEventId,
        'm.fully_read': fullyReadEventId
      })
    }
  );
  return handleApiResponse(response);
};
```

---

## 编辑/撤回消息

### 撤回消息

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/redact/{event_id}`

**需要认证:** 是

**请求体:**
```typescript
interface RedactEvent {
  reason?: string;  // 撤回原因
}
```

**请求示例:**
```typescript
const redactMessage = async (roomId: string, eventId: string, reason?: string, accessToken: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/redact/${eventId}/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        reason
      })
    }
  );
  return handleApiResponse<{
    event_id: string;
  }>(response);
};
```

---

## 打字状态

### 设置打字状态

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/typing/{user_id}`

**需要认证:** 是

**请求体:**
```typescript
interface TypingEvent {
  typing: boolean;    // 是否正在输入
  timeout?: number;   // 超时时间 (毫秒，默认 30000)
}
```

**请求示例:**
```typescript
// 开始打字
const setTyping = async (roomId: string, userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/typing/${encodeURIComponent(userId)}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        typing: true,
        timeout: 30000
      })
    }
  );
  return handleApiResponse(response);
};

// 停止打字
const stopTyping = async (roomId: string, userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/typing/${encodeURIComponent(userId)}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        typing: false
      })
    }
  );
  return handleApiResponse(response);
};
```

---

## 自定义事件

### 发送自定义事件

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`

**需要认证:** 是

**请求示例:**
```typescript
const sendCustomEvent = async (
  roomId: string,
  eventType: string,
  content: any,
  accessToken: string
) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/${encodeURIComponent(eventType)}/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(content)
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};

// 发送自定义状态事件
const setStateEvent = async (
  roomId: string,
  eventType: string,
  stateKey: string,
  content: any,
  accessToken: string
) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/v0/rooms/${encodeURIComponent(roomId)}/state/${encodeURIComponent(eventType)}/${encodeURIComponent(stateKey)}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(content)
    }
  );
  return handleApiResponse<{ event_id: string }>(response);
};
```

---

## 完整消息服务示例

```typescript
class MessageService {
  constructor(private auth: AuthService) {}

  async sendMessage(roomId: string, content: any, messageType = 'm.text') {
    const txnId = Date.now().toString();
    const response = await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/${messageType}/${txnId}`,
      {
        method: 'PUT',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify(content)
      }
    );
    return this.auth.handleResponse(response);
  }

  async sendText(roomId: string, text: string) {
    return this.sendMessage(roomId, {
      msgtype: 'm.text',
      body: text
    });
  }

  async getMessages(roomId: string, options: {
    from?: string;
    limit?: number;
    dir?: 'f' | 'b';
  } = {}) {
    const url = new URL(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/messages`);
    if (options.from) url.searchParams.set('from', options.from);
    if (options.limit) url.searchParams.set('limit', options.limit.toString());
    if (options.dir) url.searchParams.set('dir', options.dir);

    const response = await fetch(url.toString(), {
      headers: { 'Authorization': `Bearer ${this.auth.accessToken}` }
    });
    return this.auth.handleResponse(response);
  }

  async sendReadReceipt(roomId: string, eventId: string) {
    const response = await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/receipt/m.read/${eventId}`,
      {
        method: 'POST',
        headers: this.auth.getAuthHeaders()
      }
    );
    return this.auth.handleResponse(response);
  }

  async redactMessage(roomId: string, eventId: string, reason?: string) {
    const txnId = Date.now().toString();
    const response = await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/redact/${eventId}/${txnId}`,
      {
        method: 'PUT',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify({ reason })
      }
    );
    return this.auth.handleResponse(response);
  }

  async setTyping(roomId: string, isTyping: boolean) {
    const userId = this.auth.getCurrentUserId();
    if (!userId) return;

    await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/typing/${userId}`,
      {
        method: 'PUT',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify({
          typing: isTyping,
          timeout: isTyping ? 30000 : 0
        })
      }
    );
  }
}
```

---

## React Hook 示例

```typescript
import { useState, useCallback, useEffect } from 'react';

interface Message {
  event_id: string;
  sender: string;
  content: any;
  origin_server_ts: number;
}

interface UseMessagesResult {
  messages: Message[];
  loading: boolean;
  error: string | null;
  sendMessage: (text: string) => Promise<void>;
  loadMore: () => Promise<void>;
  sendReadReceipt: (eventId: string) => Promise<void>;
  setTyping: (typing: boolean) => void;
}

export function useMessages(roomId: string, accessToken: string): UseMessagesResult {
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nextToken, setNextToken] = useState<string>();

  // 加载消息
  const loadMessages = useCallback(async () => {
    setLoading(true);
    try {
      const url = new URL(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/messages`);
      if (nextToken) url.searchParams.set('from', nextToken);
      url.searchParams.set('limit', '50');
      url.searchParams.set('dir', 'f');

      const response = await fetch(url.toString(), {
        headers: { 'Authorization': `Bearer ${accessToken}` }
      });
      const result = await response.json();

      setMessages(prev => [...prev, ...result.chunk.reverse()]);
      setNextToken(result.end);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [roomId, accessToken, nextToken]);

  // 发送消息
  const sendMessage = useCallback(async (text: string) => {
    const txnId = Date.now().toString();
    const response = await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
      {
        method: 'PUT',
        headers: {
          'Authorization': `Bearer ${accessToken}`,
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          msgtype: 'm.text',
          body: text
        })
      }
    );

    if (!response.ok) {
      throw new Error('Failed to send message');
    }

    const result = await response.json();
    setMessages(prev => [...prev, result.data]);
  }, [roomId, accessToken]);

  // 发送已读回执
  const sendReadReceipt = useCallback(async (eventId: string) => {
    await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/receipt/m.read/${eventId}`,
      {
        method: 'POST',
        headers: { 'Authorization': `Bearer ${accessToken}` }
      }
    );
  }, [roomId, accessToken]);

  // 设置打字状态
  const setTyping = useCallback((typing: boolean) => {
    // 防抖处理
    if (typing) {
      fetch(
        `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/typing/${accessToken}`,
        {
          method: 'PUT',
          headers: {
            'Authorization': `Bearer ${accessToken}`,
            'Content-Type': 'application/json'
          },
          body: JSON.stringify({ typing: true, timeout: 30000 })
        }
      );
    }
  }, [roomId, accessToken]);

  return {
    messages,
    loading,
    error,
    sendMessage,
    loadMore: loadMessages,
    sendReadReceipt,
    setTyping
  };
}
```
