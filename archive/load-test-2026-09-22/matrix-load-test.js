// Matrix Load Test Script
// 使用 k6 模拟真实 Matrix 用户行为
// 用法: k6 run --vus 100 --duration 10m matrix-load-test.js
//
// 场景：用户登录、加入房间、发送消息、同步时间线
// 逐步加压：100 → 500 → 1000 → 5000 → 10000 并发用户
// 每个阶段 10-15 分钟，Prometheus 收集足够数据点

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Counter, Trend } from 'k6/metrics';

// 配置
const SYNapse_BASE_URL = __ENV.SYNAPSE_URL || 'http://localhost:8008';
const ACCESS_TOKEN = __ENV.ACCESS_TOKEN || '';
const USER_COUNT = parseInt(__ENV.USERS) || 100;

// 自定义指标
const loginDuration = new Trend('matrix_login_duration_ms');
const syncDuration = new Trend('matrix_sync_duration_ms');
const messageDuration = new Trend('matrix_message_duration_ms');
const errorRate = new Rate('matrix_error_rate');
const requestCount = new Counter('matrix_request_count');

// 默认请求选项
const headers = {
  'Content-Type': 'application/json',
  'Authorization': ACCESS_TOKEN ? `Bearer ${ACCESS_TOKEN}` : '',
};

// 用户凭证池（测试用）
const testUsers = [];
for (let i = 0; i < 1000; i++) {
  testUsers.push(`test_user_${i}`);
}

// 登录
function login(username) {
  const start = Date.now();
  const res = http.post(`${SYNapse_BASE_URL}/_matrix/client/r0/login`, {
    type: 'm.login.password',
    identifier: { type: 'm.id.user', user: username },
    password: 'test_password_123',
    device_id: `loadtest_${__VU}`,
    initial_device_display_name: 'Load Test Device',
  }, { headers });
  const duration = Date.now() - start;
  loginDuration.add(duration);
  requestCount.add(1);

  if (res.status !== 200 && res.status !== 400) {
    errorRate.add(1);
  }
  check(res, { 'login status is 200 or 400': (r) => r.status === 200 || r.status === 400 });

  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      return body.access_token;
    } catch (e) {}
  }
  return null;
}

// 创建/加入房间
function joinRoom(token) {
  const start = Date.now();
  const roomAlias = `#loadtest_room_${Math.floor(Math.random() * 100)}:localhost`;
  const res = http.post(
    `${SYNapse_BASE_URL}/_matrix/client/r0/joined_rooms`,
    { room_alias: roomAlias },
    { headers: { ...headers, 'Authorization': `Bearer ${token}` } }
  );
  const duration = Date.now() - start;
  syncDuration.add(duration);
  requestCount.add(1);
  check(res, { 'join room status is 200': (r) => r.status === 200 });
}

// 发送消息
function sendMessage(token, roomId) {
  const start = Date.now();
  const txnId = `loadtest_${Date.now()}_${Math.random()}`;
  const res = http.put(
    `${SYNapse_BASE_URL}/_matrix/client/r0/rooms/${roomId}/send/m.room.message/${txnId}`,
    {
      msgtype: 'm.text',
      body: `Load test message from VU ${__VU} at ${Date.now()}`,
    },
    { headers: { ...headers, 'Authorization': `Bearer ${token}` } }
  );
  const duration = Date.now() - start;
  messageDuration.add(duration);
  requestCount.add(1);
  check(res, { 'send message status is 200': (r) => r.status === 200 });
}

// 同步时间线 (/sync - 性能瓶颈)
function syncTimeline(token) {
  const start = Date.now();
  const res = http.get(
    `${SYNapse_BASE_URL}/_matrix/client/r0/sync?timeout=30000&since=&filter={"room":{"timeline":{"limit":20}}}`,
    { headers: { ...headers, 'Authorization': `Bearer ${token}` } }
  );
  const duration = Date.now() - start;
  syncDuration.add(duration);
  requestCount.add(1);
  check(res, { 'sync status is 200': (r) => r.status === 200 });
}

// 主场景
export default function () {
  const username = testUsers[Math.floor(Math.random() * testUsers.length)];

  // 1. 登录
  const token = login(username);
  if (!token) {
    sleep(1);
    return;
  }

  // 2. 等待同步
  sleep(1);

  // 3. 加入房间
  joinRoom(token);

  // 4. 同步时间线（性能瓶颈）
  syncTimeline(token);

  // 5. 发送消息
  const roomId = `!loadtest_${__VU}:localhost`;
  sendMessage(token, roomId);

  // 6. 再次同步
  syncTimeline(token);

  sleep(Math.random() * 2);
}

// 阈值设置
export const options = {
  stages: [
    // 逐步加压
    { duration: '10m', target: 100 },    // 100 并发用户 10 分钟
    { duration: '10m', target: 500 },    // 500 并发用户 10 分钟
    { duration: '10m', target: 1000 },   // 1000 并发用户 10 分钟
    { duration: '10m', target: 5000 },   // 5000 并发用户 10 分钟
    { duration: '10m', target: 10000 },  // 10000 并发用户 10 分钟
    { duration: '5m', target: 0 },       // 5 分钟冷却
  ],
  thresholds: {
    // 登录延迟 P95 < 500ms
    'matrix_login_duration_ms{phase:setup}': ['p(95)<500'],
    // 同步延迟 P95 < 2000ms（/sync 是瓶颈）
    'matrix_sync_duration_ms': ['p(95)<2000'],
    // 消息延迟 P95 < 500ms
    'matrix_message_duration_ms': ['p(95)<500'],
    // 错误率 < 1%
    'matrix_error_rate': ['rate<0.01'],
  },
  tags: {
    scenario: 'matrix_load_test',
  },
};

// 汇总输出
export function handleSummary(data) {
  return {
    stdout: `
=== Matrix Load Test Summary ===
  Virtual Users: ${__VU}
  Total Requests: ${requestCount.count}
  Error Rate: ${(errorRate.count / requestCount.count * 100).toFixed(2)}%
  Avg Login Duration: ${loginDuration.avg?.toFixed(0) || 'N/A'}ms
  Avg Sync Duration: ${syncDuration.avg?.toFixed(0) || 'N/A'}ms
  Avg Message Duration: ${messageDuration.avg?.toFixed(0) || 'N/A'}ms
  P95 Sync Duration: ${syncDuration.p(95)?.toFixed(0) || 'N/A'}ms
  P99 Sync Duration: ${syncDuration.p(99)?.toFixed(0) || 'N/A'}ms
===============================
    `,
    ...data,
  };
}
