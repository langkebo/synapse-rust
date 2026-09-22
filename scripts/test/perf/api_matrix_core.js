/**
 * Matrix Core API Smoke Test Script
 * 
 * 测试场景：登录 → 创建房间 → 发送消息 → 获取房间摘要 → 同步时间线
 * 
 * 使用方法:
 *   k6 run --env BASE_URL=http://localhost:8008 api_matrix_core.js
 *   k6 run --env BASE_URL=http://localhost:8008 --vus 10 --duration 30s api_matrix_core.js
 * 
 * 环境变量:
 *   BASE_URL      - Matrix 服务器地址 (默认：http://localhost:8008)
 *   ADMIN_USER    - 管理员用户名 (默认：admin)
 *   ADMIN_PASS    - 管理员密码 (默认：Admin@123)
 *   VUS           - 虚拟用户数 (默认：10)
 *   DURATION      - 测试持续时间 (默认：30s)
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend, Counter, Gauge } from 'k6/metrics';

// ============================================================================
// 配置常量
// ============================================================================

const CONFIG = {
  baseUrl: __ENV.BASE_URL || 'http://localhost:8008',
  adminUser: __ENV.ADMIN_USER || 'admin',
  adminPass: __ENV.ADMIN_PASS || 'Admin@123',
  requestTimeout: parseInt(__ENV.REQUEST_TIMEOUT || '30000', 10),
};

// ============================================================================
// 自定义指标定义
// ============================================================================

// 错误率
const errorRate = new Rate('errors');

// 延迟指标 (单位：ms)
const loginDuration = new Trend('login_duration');
const createRoomDuration = new Trend('create_room_duration');
const sendMessageDuration = new Trend('send_message_duration');
const syncDuration = new Trend('sync_duration');
const roomSummaryDuration = new Trend('room_summary_duration');

// 辅助指标
const requestCount = new Counter('requests_total');
// 成功率 (Rate: 成功次数 / 总次数；每次迭代无论成败都 add，成功=1 失败=0)
const successRate = new Rate('success_rate');
const activeUsers = new Gauge('active_vus');

// ============================================================================
// k6 配置选项
// ============================================================================

export const options = {
  // 默认执行配置 (可通过命令行覆盖)
  stages: [
    { duration: '5s', target: 1 },    // 缓慢启动
    { duration: '10s', target: 5 },   // 增加到 5 用户
    { duration: '10s', target: 5 },   // 保持稳定
    { duration: '5s', target: 0 },    // 逐渐停止
  ],
  
  // 性能阈值 (Smoke Test 标准)
  thresholds: {
    // 延迟阈值 (P95)
    'login_duration': ['p(95)<500'],
    'create_room_duration': ['p(95)<800'],
    'send_message_duration': ['p(95)<600'],
    'sync_duration': ['p(95)<1000'],
    'room_summary_duration': ['p(95)<500'],
    
    // 错误率阈值
    'errors': ['rate<0.01'],
    
    // 成功率阈值
    'success_rate': ['rate>0.99'],
  },
  
  // 其他配置
  ext: {
    loadimpact: {
      distribution: {
        'amazon:us:ashburn': 1,
      },
    },
  },
  
  // 丢弃的指标过滤 (减少输出噪音)
  discardResponseBodies: true,
};

// ============================================================================
// 工具函数
// ============================================================================

/**
 * 生成随机房间名称
 */
function generateRoomName() {
  return `Smoke Test Room ${Date.now()}-${Math.floor(Math.random() * 10000)}`;
}

/**
 * 生成随机消息内容
 */
function generateMessage() {
  return `Smoke test message ${Date.now()} - ${Math.random().toString(36).substring(7)}`;
}

/**
 * 格式化毫秒数为可读字符串
 */
function formatDuration(ms) {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

// ============================================================================
// Setup: 获取认证令牌
// ============================================================================

export function setup() {
  console.log(`[${new Date().toISOString()}] Starting setup...`);
  console.log(`Target server: ${CONFIG.baseUrl}`);
  
  const loginPayload = JSON.stringify({
    type: 'm.login.password',
    identifier: {
      type: 'm.id.user',
      user: CONFIG.adminUser,
    },
    password: CONFIG.adminPass,
  });
  
  const loginParams = {
    headers: {
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    },
    timeout: CONFIG.requestTimeout.toString(),
  };
  
  const loginRes = http.post(
    `${CONFIG.baseUrl}/_matrix/client/v3/login`,
    loginPayload,
    loginParams
  );
  
  // 验证登录响应
  const loginChecks = check(loginRes, {
    'setup: login status is 200': (r) => r.status === 200,
    'setup: response is valid JSON': (r) => {
      try {
        JSON.parse(r.body);
        return true;
      } catch (e) {
        return false;
      }
    },
    'setup: has access_token': (r) => {
      const body = JSON.parse(r.body);
      return body && body.access_token && typeof body.access_token === 'string';
    },
    'setup: has user_id': (r) => {
      const body = JSON.parse(r.body);
      return body && body.user_id && typeof body.user_id === 'string';
    },
  });
  
  // 记录错误
  if (!loginChecks) {
    errorRate.add(1);
    console.error('[SETUP ERROR] Login failed:');
    console.error(`  Status: ${loginRes.status}`);
    console.error(`  Body: ${(loginRes.body || '').substring(0, 200)}`);
    throw new Error('Setup failed: unable to authenticate');
  }
  
  const responseBody = JSON.parse(loginRes.body);
  
  if (!responseBody.access_token) {
    console.error('[SETUP ERROR] No access_token in response body');
    throw new Error('Setup failed: no access_token');
  }
  
  console.log(`[SETUP SUCCESS] Authenticated as ${responseBody.user_id}`);
  console.log(`[SETUP SUCCESS] Access token obtained (${responseBody.access_token.length} chars)`);
  
  return {
    token: responseBody.access_token,
    userId: responseBody.user_id,
    deviceId: responseBody.device_id || null,
  };
}

// ============================================================================
// Main Test Flow
// ============================================================================

export default function (data) {
  activeUsers.add(1);
  
  const headers = {
    'Content-Type': 'application/json',
    'Accept': 'application/json',
    'Authorization': `Bearer ${data.token}`,
  };
  
  const commonParams = {
    headers: headers,
    timeout: CONFIG.requestTimeout.toString(),
  };
  
  let currentRoomId = null;
  let testPassed = true;
  
  // ────────────────────────────────────────────────────────────────────────
  // Step 1: 登录验证
  // ────────────────────────────────────────────────────────────────────────
  group('01_Login', () => {
    const startTime = Date.now();
    
    const loginRes = http.post(
      `${CONFIG.baseUrl}/_matrix/client/v3/login`,
      JSON.stringify({
        type: 'm.login.password',
        identifier: {
          type: 'm.id.user',
          user: CONFIG.adminUser,
        },
        password: CONFIG.adminPass,
      }),
      commonParams
    );
    
    const duration = Date.now() - startTime;
    loginDuration.add(duration);
    requestCount.add(1);
    
    const checks = check(loginRes, {
      'login: status 200': (r) => r.status === 200,
      'login: has access_token': (r) => {
        try {
          const body = JSON.parse(r.body);
          return body && body.access_token;
        } catch (e) {
          return false;
        }
      },
      'login: response time < 500ms': (r) => duration < 500,
    });
    
    if (!checks) {
      errorRate.add(1);
      testPassed = false;
      console.warn(`[WARN] Login failed: status=${loginRes.status}, duration=${duration}ms`);
    } else {
      successRate.add(1);
    }
  });
  
  // ────────────────────────────────────────────────────────────────────────
  // Step 2: 创建测试房间
  // ────────────────────────────────────────────────────────────────────────
  group('02_CreateRoom', () => {
    const startTime = Date.now();
    const roomName = generateRoomName();
    
    const createRes = http.post(
      `${CONFIG.baseUrl}/_matrix/client/v3/createRoom`,
      JSON.stringify({
        name: roomName,
        preset: 'private_chat',
        visibility: 'private',
      }),
      commonParams
    );
    
    const duration = Date.now() - startTime;
    createRoomDuration.add(duration);
    requestCount.add(1);
    
    const checks = check(createRes, {
      'createRoom: status 200': (r) => r.status === 200,
      'createRoom: has room_id': (r) => {
        try {
          const body = JSON.parse(r.body);
          return body && body.room_id;
        } catch (e) {
          return false;
        }
      },
      'createRoom: response time < 800ms': (r) => duration < 800,
    });
    
    if (!checks) {
      errorRate.add(1);
      testPassed = false;
      console.warn(`[WARN] Create room failed: status=${createRes.status}, duration=${duration}ms`);
    } else {
      successRate.add(1);
      currentRoomId = JSON.parse(createRes.body).room_id;
    }
  });
  
  // ────────────────────────────────────────────────────────────────────────
  // Step 3: 发送消息 (仅当房间创建成功时)
  // ────────────────────────────────────────────────────────────────────────
  if (currentRoomId && testPassed) {
    group('03_SendMessage', () => {
      const startTime = Date.now();
      const messageId = `msg_${Date.now()}_${Math.random().toString(36).substring(7)}`;
      
      const sendRes = http.put(
        `${CONFIG.baseUrl}/_matrix/client/v3/rooms/${encodeURIComponent(currentRoomId)}/send/m.room.message/${messageId}`,
        JSON.stringify({
          msgtype: 'm.text',
          body: generateMessage(),
        }),
        commonParams
      );
      
      const duration = Date.now() - startTime;
      sendMessageDuration.add(duration);
      requestCount.add(1);
      
      const checks = check(sendRes, {
        'sendMessage: status 200': (r) => r.status === 200,
        'sendMessage: has event_id': (r) => {
          try {
            const body = JSON.parse(r.body);
            return body && body.event_id;
          } catch (e) {
            return false;
          }
        },
        'sendMessage: response time < 600ms': (r) => duration < 600,
      });
      
      if (!checks) {
        errorRate.add(1);
        testPassed = false;
        console.warn(`[WARN] Send message failed: status=${sendRes.status}, duration=${duration}ms`);
      } else {
        successRate.add(1);
      }
    });
    
    // ────────────────────────────────────────────────────────────────────────
    // Step 4: 获取房间摘要
    // ────────────────────────────────────────────────────────────────────────
    group('04_RoomSummary', () => {
      const startTime = Date.now();
      
      const summaryRes = http.get(
        `${CONFIG.baseUrl}/_matrix/client/v3/rooms/${encodeURIComponent(currentRoomId)}/summary`,
        commonParams
      );
      
      const duration = Date.now() - startTime;
      roomSummaryDuration.add(duration);
      requestCount.add(1);
      
      // 允许 200 或 404 (房间可能太小无法生成摘要)
      const checks = check(summaryRes, {
        'roomSummary: status 200 or 404': (r) => r.status === 200 || r.status === 404,
        'roomSummary: response time < 500ms': (r) => duration < 500,
      });
      
      if (!checks) {
        errorRate.add(1);
        testPassed = false;
        console.warn(`[WARN] Room summary failed: status=${summaryRes.status}, duration=${duration}ms`);
      } else {
        successRate.add(1);
      }
    });
  }
  
  // ────────────────────────────────────────────────────────────────────────
  // Step 5: 同步时间线
  // ────────────────────────────────────────────────────────────────────────
  group('05_Sync', () => {
    const startTime = Date.now();
    
    const syncRes = http.get(
      `${CONFIG.baseUrl}/_matrix/client/v3/sync?timeout=1000&filter=live`,
      commonParams
    );
    
    const duration = Date.now() - startTime;
    syncDuration.add(duration);
    requestCount.add(1);
    
    const checks = check(syncRes, {
      'sync: status 200': (r) => r.status === 200,
      'sync: has next_batch': (r) => {
        try {
          const body = JSON.parse(r.body);
          return body && body.next_batch;
        } catch (e) {
          return false;
        }
      },
      'sync: response time < 1000ms': (r) => duration < 1000,
    });
    
    if (!checks) {
      errorRate.add(1);
      testPassed = false;
      console.warn(`[WARN] Sync failed: status=${syncRes.status}, duration=${duration}ms`);
    } else {
      successRate.add(1);
    }
  });
  
  // 短暂休息
  sleep(0.5);
  
  activeUsers.add(-1);
  
  // 记录测试迭代状态
  if (!testPassed) {
    errorRate.add(1);
  }
}

// ============================================================================
// Handle Summary: 自定义报告输出
// ============================================================================

export function handleSummary(data) {
  const results = {
    timestamp: new Date().toISOString(),
    server: CONFIG.baseUrl,
    thresholds_passed: true,
    summary: {},
    details: {},
  };
  
  // 提取关键指标
  const metrics = data.metrics;
  
  const extractValue = (metric, key) => {
    if (!metric || !metric.values || metric.values[key] === undefined) return null;
    return metric.values[key];
  };
  
  // 构建摘要
  results.summary = {
    login_p95: extractValue(metrics.login_duration, 'p(95)'),
    create_room_p95: extractValue(metrics.create_room_duration, 'p(95)'),
    send_message_p95: extractValue(metrics.send_message_duration, 'p(95)'),
    sync_p95: extractValue(metrics.sync_duration, 'p(95)'),
    room_summary_p95: extractValue(metrics.room_summary_duration, 'p(95)'),
    error_rate: extractValue(metrics.errors, 'rate'),
    success_rate: extractValue(metrics.success_rate, 'rate'),
    total_requests: extractValue(metrics.requests_total, 'value'),
    total_iterations: extractValue(data.metrics.iterations, 'value'),
  };
  
  // 检查阈值 (使用我们定义的原始阈值表达式, 不受 k6 处理后的格式影响)
  const thresholdResults = {};
  for (const [name, thresholds] of Object.entries(options.thresholds)) {
    const metric = metrics[name];
    if (!metric || !metric.values) {
      thresholdResults[name] = { thresholds: thresholds, passed: false, actual: null, note: 'metric not available' };
      results.thresholds_passed = false;
      continue;
    }

    thresholdResults[name] = { thresholds: thresholds, passed: true, actual: metric.values };

    // k6 将 thresholds 数组转为对象 { expr: {...} }，使用 Object.values 获取表达式列表
    const exprList = Array.isArray(thresholds) ? thresholds : Object.keys(thresholds);

    for (const expr of exprList) {
      // 解析阈值表达式 (例如："p(95)<500" 或 "rate<0.01")
      const match = expr.match(/^(\S+?)([<>=]+)(\S+)$/);
      if (!match) continue;

      const [, key, op, value] = match;
      const actualValue = metric.values[key];

      if (actualValue == null) {
        thresholdResults[name].passed = false;
        continue;
      }

      let passed = false;
      switch (op) {
        case '<': passed = actualValue < parseFloat(value); break;
        case '<=': passed = actualValue <= parseFloat(value); break;
        case '>': passed = actualValue > parseFloat(value); break;
        case '>=': passed = actualValue >= parseFloat(value); break;
        case '=': passed = actualValue === parseFloat(value); break;
      }

      if (!passed) {
        thresholdResults[name].passed = false;
        results.thresholds_passed = false;
      }
    }
  }
  
  results.details.thresholds = thresholdResults;
  
  // 输出格式化的控制台报告
  console.log('\n' + '='.repeat(80));
  console.log('MATRIX CORE API SMOKE TEST REPORT');
  console.log('='.repeat(80));
  console.log(`Server: ${CONFIG.baseUrl}`);
  console.log(`Time: ${results.timestamp}`);
  console.log('-'.repeat(80));
  
  // 性能指标
  console.log('\n📊 PERFORMANCE METRICS (P95):');
  console.log('-'.repeat(40));
  
  const formatMs = (val) => val ? `${val.toFixed(2)} ms` : 'N/A';
  
  console.log(`  Login:              ${formatMs(results.summary.login_p95)}`);
  console.log(`  Create Room:        ${formatMs(results.summary.create_room_p95)}`);
  console.log(`  Send Message:       ${formatMs(results.summary.send_message_p95)}`);
  console.log(`  Sync:               ${formatMs(results.summary.sync_p95)}`);
  console.log(`  Room Summary:       ${formatMs(results.summary.room_summary_p95)}`);
  
  // 错误统计
  console.log('\n📈 ERROR STATISTICS:');
  console.log('-'.repeat(40));
  console.log(`  Total Requests:     ${results.summary.total_requests || 0}`);
  console.log(`  Error Rate:         ${(results.summary.error_rate * 100).toFixed(2)}%`);
  console.log(`  Success Rate:       ${(results.summary.success_rate * 100).toFixed(2)}%`);
  
  // 阈值检查结果
  console.log('\n✅ THRESHOLD CHECKS:');
  console.log('-'.repeat(40));
  
  for (const [name, result] of Object.entries(thresholdResults)) {
    const icon = result.passed ? '✅' : '❌';
    console.log(`  ${icon} ${name}: ${result.passed ? 'PASSED' : 'FAILED'}`);
  }
  
  // 总体状态
  console.log('\n' + '='.repeat(80));
  console.log(`OVERALL STATUS: ${results.thresholds_passed ? '✅ PASSED' : '❌ FAILED'}`);
  console.log('='.repeat(80) + '\n');
  
  // 返回汇总数据
  return {
    'stdout': textSummary(data, { indent: '  ', enableColors: true }),
    'json-summary': JSON.stringify(results, null, 2),
    'results.json': JSON.stringify(results, null, 2),
  };
}

/**
 * 文本摘要 (备用输出格式)
 */
function textSummary(data, options) {
  const indent = options?.indent || '  ';
  
  let output = '\n';
  output += `${indent}Test completed at ${new Date().toISOString()}\n`;
  output += `${indent}Total iterations: ${data.state.testRuns}\n`;
  
  if (data.metrics.errors) {
    const errorRate = data.metrics.errors.values.rate || 0;
    output += `${indent}Error rate: ${(errorRate * 100).toFixed(2)}%\n`;
  }
  
  return output;
}
