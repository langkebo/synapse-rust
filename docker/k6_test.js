// k6 性能测试脚本
// synapse-rust - API 基准测试
// 运行: k6 run k6_test.js

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend } from 'k6/metrics';

// 自定义指标
const errorRate = new Rate('errors');
const loginDuration = new Trend('login_duration');
const sendMessageDuration = new Trend('send_message_duration');
const roomCreateDuration = new Trend('room_create_duration');

// 配置
const BASE_URL = __ENV.BASE_URL || 'http://localhost:8008';
const ADMIN_USER = __ENV.ADMIN_USER || 'admin';
const ADMIN_PASS = __ENV.ADMIN_PASS || 'Admin@123';

// 测试配置
export const options = {
  scenarios: {
    // 峰值测试: 1000 并发
    peak_load: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: 100 },   // 30s 内增加到 100 用户
        { duration: '1m', target: 500 },    // 1min 内增加到 500
        { duration: '2m', target: 1000 },   // 2min 内增加到 1000
        { duration: '30s', target: 0 },      // 30s 内降为 0
      ],
      gracefulRampDown: '30s',
    },
    // 持续测试
    soak_test: {
      executor: 'constant-vus',
      vus: 500,
      duration: '10m',
    },
  },
  thresholds: {
    // 性能目标
    http_req_duration: ['p(99)<=200'],  // P99 ≤ 200ms
    http_req_failed: ['rate<0.01'],    // 错误率 < 1%
    errors: ['rate<0.05'],              // 错误率 < 5%
  },
};

// 全局变量
let adminAccessToken = '';
let testUserToken = '';
let testRoomId = '';

export function setup() {
  // 管理员登录
  const loginRes = http.post(
    `${BASE_URL}/_matrix/client/v3/login`,
    JSON.stringify({
      type: 'm.login.password',
      identifier: {
        type: 'm.id.user',
        user: ADMIN_USER,
      },
      password: ADMIN_PASS,
    }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const loginData = JSON.parse(loginRes.body);
  adminAccessToken = loginData.access_token;

  // 创建测试用户
  const testUser = `test_${Date.now()}`;
  const createRes = http.post(
    `${BASE_URL}/_synapse/admin/v2/users/${testUser}`,
    JSON.stringify({
      password: 'TestPass123!',
      displayname: testUser,
    }),
    {
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${adminAccessToken}`,
      },
    }
  );

  // 测试用户登录
  const testLoginRes = http.post(
    `${BASE_URL}/_matrix/client/v3/login`,
    JSON.stringify({
      type: 'm.login.password',
      identifier: {
        type: 'm.id.user',
        user: testUser,
      },
      password: 'TestPass123!',
    }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const testLoginData = JSON.parse(testLoginRes.body);
  testUserToken = testLoginData.access_token;

  return { adminToken: adminAccessToken, userToken: testUserToken };
}

export default function (data) {
  // 测试组 1: 用户认证
  group('Authentication', () => {
    // 登录性能测试 (已在 setup 中完成，这里仅做验证)
    check(data.adminToken, {
      'admin token exists': (t) => t !== '',
    });
  });

  // 测试组 2: 房间管理
  group('Room Management', () => {
    // 创建房间
    const createRoomRes = http.post(
      `${BASE_URL}/_matrix/client/v3/createRoom`,
      JSON.stringify({
        room_alias_name: `test_${Date.now()}`,
        name: 'Performance Test Room',
        topic: 'K6 Load Test',
        preset: 'private_chat',
      }),
      {
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${data.userToken}`,
        },
      }
    );

    roomCreateDuration.add(createRoomRes.timings.duration);
    check(createRoomRes, {
      'room created': (r) => r.status === 200,
      'room has id': (r) => JSON.parse(r.body).room_id !== undefined,
    });

    if (createRoomRes.status === 200) {
      testRoomId = JSON.parse(createRoomRes.body).room_id;
    }
  });

  // 测试组 3: 消息发送
  group('Messaging', () => {
    if (testRoomId) {
      const sendRes = http.post(
        `${BASE_URL}/_matrix/client/v3/rooms/${testRoomId}/send/m.room.message`,
        JSON.stringify({
          msgtype: 'm.text',
          body: `Test message at ${Date.now()}`,
        }),
        {
          headers: {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${data.userToken}`,
            'X-Request-ID': `k6-${Date.now()}-${__VU}`,
          },
        }
      );

      sendMessageDuration.add(sendRes.timings.duration);
      check(sendRes, {
        'message sent': (r) => r.status === 200,
      });
      errorRate.add(sendRes.status !== 200);
    }
  });

  // 测试组 4: 同步
  group('Sync', () => {
    const syncRes = http.get(
      `${BASE_URL}/_matrix/client/v3/sync?timeout=10000`,
      {
        headers: {
          'Authorization': `Bearer ${data.userToken}`,
        },
      }
    );

    loginDuration.add(syncRes.timings.duration);
    check(syncRes, {
      'sync successful': (r) => r.status === 200,
    });
    errorRate.add(syncRes.status !== 200);
  });

  sleep(1);
}

export function handleSummary(data) {
  return {
    'stdout': textSummary(data, { indent: ' ', enableColors: true }),
    'summary.json': JSON.stringify(data),
  };
}

// 文本摘要格式化
function textSummary(data, opts) {
  const indent = opts.indent || '';
  let output = `\n${indent}=== Performance Test Summary ===\n\n`;
  
  output += `${indent}Total Requests: ${data.metrics.http_reqs.values.count}\n`;
  output += `${indent}Failed Requests: ${data.metrics.http_req_failed.values.passes}\n`;
  output += `${indent}Request Duration (P99): ${data.metrics.http_req_duration.values['p(99)']}ms\n`;
  output += `${indent}Errors Rate: ${(data.metrics.errors.values.rate * 100).toFixed(2)}%\n`;
  
  return output;
}
