#!/usr/bin/env node
/**
 * Matrix IM Core API Performance Test (k6)
 * 
 * 用途：验证核心接口在负载下的性能表现
 * 用法:
 *   k6 run --vus 10 --duration 1m api_matrix_core.js
 *   k6 run --vus 50 --duration 5m api_matrix_core.js
 *   k6 run --out json=results.json api_matrix_core.js
 * 
 * 依赖：k6 v0.45+
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { group } from 'k6';
import { Rate } from 'k6/metrics';

// ========== 配置区 ==========
export const options = {
  stages: [
    { duration: '30s', target: 10 },   // 缓慢增加到 10 并发
    { duration: '1m', target: 10 },    // 保持 10 并发 1 分钟
    { duration: '30s', target: 20 },   // 增加到 20 并发
    { duration: '2m', target: 20 },    // 保持 20 并发 2 分钟
    { duration: '30s', target: 0 },    // 逐渐降为 0
  ],
  thresholds: {
    http_req_failed: ['rate < 0.01'],  // 错误率 < 1%
    http_req_duration: ['p(95)<200', 'p(99)<500'],  // P95 < 200ms, P99 < 500ms
  },
};

// ========== 自定义指标 ==========
const errorRate = new Rate('errors');

// ========== 测试配置 ==========
const BASE_URL = __ENV.BASE_URL || 'http://localhost:8008';
const USERNAME = __ENV.TEST_USERNAME || 'admin';
const PASSWORD = __ ENV.TEST_PASSWORD || 'Admin@123';

// ========== 全局变量 ==========
let authTokens = {};

// ========== 辅助函数 ==========
function randomString(length) {
  return Math.random().toString(36).substring(2, 2 + length);
}

// ========== 测试场景 ==========

// 1. 登录测试
function testLogin() {
  const userId = `test_${randomString(8)}`;
  
  group('1. Login', function () {
    // 注册新用户（如果需要）
    let res = http.post(`${BASE_URL}/_matrix/client/r0/register`, JSON.stringify({
      username: userId,
      password: PASSWORD,
      kind: 'm.login.password',
    }), {
      headers: { 'Content-Type': 'application/json' },
    });
    
    // 登录（优先使用注册的用户，失败则用 admin）
    res = http.post(`${BASE_URL}/_matrix/client/r0/login`, JSON.stringify({
      type: 'm.login.password',
      identifier: {
        type: 'm.id.user',
        user: userId,
      },
      password: PASSWORD,
    }), {
      headers: { 'Content-Type': 'application/json' },
    });
    
    const loginSuccess = check(res, {
      'login status is 200': (r) => r.status === 200,
      'has access_token': (r) => JSON.parse(r.body).access_token !== undefined,
    });
    
    errorRate.add(!loginSuccess);
    
    if (loginSuccess && res.status === 200) {
      const body = JSON.parse(res.body);
      authTokens[userId] = {
        token: body.access_token,
        userId: body.user_id,
      };
    } else {
      // 使用 admin 账户登录
      res = http.post(`${BASE_URL}/_matrix/client/r0/login`, JSON.stringify({
        type: 'm.login.password',
        identifier: {
          type: 'm.id.user',
          user: USERNAME,
        },
        password: PASSWORD,
      }), {
        headers: { 'Content-Type': 'application/json' },
      });
      
      const adminSuccess = check(res, {
        'admin login status is 200': (r) => r.status === 200,
      });
      
      errorRate.add(!adminSuccess);
      
      if (adminSuccess) {
        authTokens.admin = {
          token: JSON.parse(res.body).access_token,
          userId: JSON.parse(res.body).user_id,
        };
      }
    }
    
    sleep(1);
  });
}

// 2. 创建房间测试
function testCreateRoom() {
  const token = Object.values(authTokens)[0]?.token;
  if (!token) return null;
  
  let roomId = null;
  
  group('2. Create Room', function () {
    const res = http.post(`${BASE_URL}/_matrix/client/r0/createRoom`, JSON.stringify({
      visibility: 'private',
      room_alias_name: `test_room_${randomString(6)}`,
      name: `Test Room ${randomString(8)}`,
    }), {
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
    });
    
    const createSuccess = check(res, {
      'create room status is 200': (r) => r.status === 200,
      'has room_id': (r) => JSON.parse(res.body).room_id !== undefined,
    });
    
    errorRate.add(!createSuccess);
    
    if (createSuccess) {
      roomId = JSON.parse(res.body).room_id;
    }
    
    sleep(1);
  });
  
  return roomId;
}

// 3. 发送消息测试
function testSendMessage(roomId, token) {
  if (!roomId || !token) return;
  
  group('3. Send Message', function () {
    const res = http.put(
      `${BASE_URL}/_matrix/client/r0/rooms/${roomId}/send/m.room.message/${randomString(20)}`,
      JSON.stringify({
        msgtype: 'm.text',
        body: `Test message at ${Date.now()}`,
      }),
      {
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`,
        },
      }
    );
    
    const sendSuccess = check(res, {
      'send message status is 200': (r) => r.status === 200,
    });
    
    errorRate.add(!sendSuccess);
    sleep(0.5);
  });
}

// 4. Sync 测试
function testSync(token) {
  if (!token) return;
  
  group('4. Sync', function () {
    const res = http.get(`${BASE_URL}/_matrix/client/r0/sync`, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });
    
    const syncSuccess = check(res, {
      'sync status is 200': (r) => r.status === 200,
      'has next_batch': (r) => JSON.parse(res.body).next_batch !== undefined,
    });
    
    errorRate.add(!syncSuccess);
    sleep(1);
  });
}

// 5. 获取房间状态测试
function testRoomState(roomId, token) {
  if (!roomId || !token) return;
  
  group('5. Room State', function () {
    const res = http.get(`${BASE_URL}/_matrix/client/r0/rooms/${roomId}/state`, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });
    
    const stateSuccess = check(res, {
      'room state status is 200': (r) => r.status === 200,
    });
    
    errorRate.add(!stateSuccess);
    sleep(0.5);
  });
}

// ========== 主测试流程 ==========
export default function () {
  // Step 1: 登录获取 token
  testLogin();
  
  // Step 2: 创建房间
  const roomId = testCreateRoom();
  
  if (!roomId) {
    sleep(5);
    return;
  }
  
  // Step 3-5: 循环执行核心操作
  for (let i = 0; i < 3; i++) {
    const token = Object.values(authTokens)[0]?.token;
    if (!token) break;
    
    testSendMessage(roomId, token);
    testSync(token);
    testRoomState(roomId, token);
    
    sleep(2);
  }
  
  // Cleanup
  sleep(2);
}

// ========== 自定义输出 ==========
export function handleSummary(data) {
  return {
    'stdout': textSummary(data, { indent: ' ', enableColors: true }),
    'results.json': JSON.stringify(data),
  };
}

function textSummary(data, options) {
  const { indent = '', enableColors = false } = options;
  
  let output = `\n${indent}===== Performance Test Summary =====\n`;
  output += `${indent}  Duration: ${data.state.testRunDurationMs / 1000}s\n`;
  output += `${indent}  Requests: ${data.metrics.http_reqs.values.count}\n`;
  output += `${indent}  Errors: ${(data.metrics.http_req_failed.values.rate * 100).toFixed(2)}%\n`;
  output += `${indent}  P50: ${Math.round(data.metrics.http_req_duration.values['p(50)'])}ms\n`;
  output += `${indent}  P95: ${Math.round(data.metrics.http_req_duration.values['p(95)'])}ms\n`;
  output += `${indent}  P99: ${Math.round(data.metrics.http_req_duration.values['p(99)'])}ms\n`;
  output += `${indent}=================================\n`;
  
  return output;
}
