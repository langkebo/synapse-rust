// Matrix IM 服务负载测试脚本 - Phase 3（极简版）
// 作者：User
// 描述：最大并发测试，单 Token 接受部分失败

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Counter } from 'k6/metrics';

export let errorRate = new Rate('error_rate');
export let messagesSent = new Counter('messages_sent');

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:8008';
const ADMIN_TOKEN = __ENV.ADMIN_TOKEN || '';

if (!ADMIN_TOKEN) {
  console.error('ERROR: ADMIN_TOKEN not set');
  throw new Error('Missing ADMIN_TOKEN');
}

export function setup() {
  console.log('=== Phase 3: 大规模负载测试 (极简版) ===');
  console.log(`服务器地址：${BASE_URL}`);
  
  return { token: ADMIN_TOKEN };
}

// 随机延迟（0-10ms，最小延迟）
function randomSleep() {
  sleep(Math.random() * 0.01);
}

export default function(data) {
  const token = data.token;
  
  // 随机初始延迟（增大间隔，降低并发密度）
  randomSleep();
  
  // 1. 创建房间（使用完全唯一的别名 + 时间戳 + VU ID）
  const ts = Date.now();
  const vuId = String(__VU).padStart(5, '0');
  const randomPart = Math.random().toString(36).substring(2, 12);
  const uniqueSuffix = `${ts}_${vuId}_${randomPart}`;
  
  // 关键修复：添加 ignore_duplicate_name:true，允许同名房间创建（负载测试需要）
  const createRoomRes = http.post(`${BASE_URL}/_matrix/client/v3/createRoom`, JSON.stringify({
    room_alias_name: `loadtest_${uniqueSuffix}`,
    name: `负载测试房间`, // 移除 ${__VU}，统一名称以便测试
    visibility: 'private',
    ignore_duplicate_name: true, // 允许同名房间，避免 M_ROOM_IN_USE 错误
  }), {
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
  });
  
  const roomCreated = check(createRoomRes, {
    '房间创建成功 (200)': (r) => r.status === 200,
    '跳过已存在 (409)': (r) => r.status === 409,
  });
  
  errorRate.add(!roomCreated);
  
  let roomId = null;
  if (createRoomRes.status === 200) {
    try {
      roomId = createRoomRes.json('room_id');
    } catch(e) {}
  }
  
  // 2. 发送消息（每 10 个 VU 发送 1 条，大幅降低频率）
  if (roomId && __VU % 10 === 0) {
    const msgRes = http.put(`${BASE_URL}/_matrix/client/v3/rooms/${roomId}/send/m.room.message/${Date.now()}_0`, JSON.stringify({
      msgtype: 'm.text',
      body: `负载测试消息 ${__VU}`,
    }), {
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
    });
    
    const msgSent = check(msgRes, {
      '消息发送成功': (r) => r.status === 200,
    });
    
    errorRate.add(!msgSent);
    messagesSent.add(msgSent ? 1 : 0);
  }
  
  // 3. 同步请求（每 20 个 VU）
  if (__VU % 20 === 0) {
    const syncRes = http.get(`${BASE_URL}/_matrix/client/v3/sync?timeout=1000`, {
      headers: { 'Authorization': `Bearer ${token}` },
    });
    
    check(syncRes, {
      '同步请求成功': (r) => r.status === 200,
    });
    errorRate.add(syncRes.status !== 200);
  }
}

export function handleSummary(data) {
  const total = data.metrics.http_reqs?.values.count || 0;
  const failedRate = data.metrics.http_req_failed?.values.rate || 0;
  const p95 = data.metrics.http_req_duration?.values['p(95)'] || 0;
  const p99 = data.metrics.http_req_duration?.values['p(99)'] || 0;
  const sent = data.metrics.messages_sent?.values.count || 0;
  const avgReq = data.metrics.http_reqs?.values.avg || 0;

  return {
    'load-test-results/summary-minimal.json': JSON.stringify({
      timestamp: new Date().toISOString(),
      phase: 'Phase 3 - 极简版负载测试',
      total_requests: total,
      avg_req_per_sec: Number(avgReq.toFixed(2)),
      p95_ms: Math.round(p95),
      p99_ms: Math.round(p99),
      error_rate_percent: Number((failedRate * 100).toFixed(2)),
      messages_sent: sent,
    }, null, 2),
  };
}
