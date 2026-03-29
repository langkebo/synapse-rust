import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8008';
const ADMIN_USER = __ENV.ADMIN_USER || 'admin';
const ADMIN_PASS = __ENV.ADMIN_PASS || 'Admin@123';

const errorRate = new Rate('errors');
const loginDuration = new Trend('login_duration');
const createRoomDuration = new Trend('create_room_duration');
const sendMessageDuration = new Trend('send_message_duration');
const syncDuration = new Trend('sync_duration');
const roomSummaryDuration = new Trend('room_summary_duration');

let authToken = '';
let testRoomId = '';

export const options = {
  stages: [
    { duration: '30s', target: 10 },
    { duration: '1m', target: 50 },
    { duration: '1m', target: 100 },
    { duration: '1m', target: 200 },
    { duration: '30s', target: 0 },
  ],
  thresholds: {
    'login_duration': ['p(95)<500'],
    'create_room_duration': ['p(95)<800'],
    'send_message_duration': ['p(95)<600'],
    'sync_duration': ['p(95)<1000'],
    'room_summary_duration': ['p(95)<500'],
    'errors': ['rate<0.01'],
  },
};

export function setup() {
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

  check(loginRes, {
    'login successful': (r) => r.status === 200,
    'has access token': (r) => JSON.parse(r.body).access_token !== undefined,
  });

  return {
    token: JSON.parse(loginRes.body).access_token,
    userId: JSON.parse(loginRes.body).user_id,
  };
}

export default function (data) {
  const headers = {
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${data.token}`,
  };

  group('Login', () => {
    const start = Date.now();
    const res = http.post(
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
    loginDuration.add(Date.now() - start);

    check(res, {
      'login status 200': (r) => r.status === 200,
      'has access token': (r) => JSON.parse(r.body).access_token !== undefined,
    });
    errorRate.add(res.status !== 200);
  });

  group('Create Room', () => {
    const start = Date.now();
    const res = http.post(
      `${BASE_URL}/_matrix/client/v3/createRoom`,
      JSON.stringify({
        preset: 'public_chat',
        room_name: `Perf Test Room ${Date.now()}`,
      }),
      { headers }
    );
    createRoomDuration.add(Date.now() - start);

    if (res.status === 200) {
      const body = JSON.parse(res.body);
      if (body.room_id) {
        testRoomId = body.room_id;
      }
    }

    check(res, {
      'create room status 200': (r) => r.status === 200,
      'has room_id': (r) => JSON.parse(r.body).room_id !== undefined,
    });
    errorRate.add(res.status !== 200);
  });

  if (testRoomId) {
    group('Send Message', () => {
      const start = Date.now();
      const res = http.put(
        `${BASE_URL}/_matrix/client/v3/rooms/${encodeURIComponent(testRoomId)}/send/m.room.message/${Date.now()}`,
        JSON.stringify({
          msgtype: 'm.text',
          body: `Performance test message at ${new Date().toISOString()}`,
        }),
        { headers }
      );
      sendMessageDuration.add(Date.now() - start);

      check(res, {
        'send message status 200': (r) => r.status === 200,
        'has event_id': (r) => JSON.parse(r.body).event_id !== undefined,
      });
      errorRate.add(res.status !== 200);
    });

    group('Room Summary', () => {
      const start = Date.now();
      const res = http.get(
        `${BASE_URL}/_matrix/client/v3/rooms/${encodeURIComponent(testRoomId)}/summary`,
        { headers }
      );
      roomSummaryDuration.add(Date.now() - start);

      check(res, {
        'room summary status 200': (r) => r.status === 200 || r.status === 404,
      });
      errorRate.add(res.status !== 200 && res.status !== 404);
    });
  }

  group('Sync', () => {
    const start = Date.now();
    const res = http.get(
      `${BASE_URL}/_matrix/client/v3/sync?timeout=1000`,
      { headers }
    );
    syncDuration.add(Date.now() - start);

    check(res, {
      'sync status 200': (r) => r.status === 200,
      'has next_batch': (r) => JSON.parse(r.body).next_batch !== undefined,
    });
    errorRate.add(res.status !== 200);
  });

  sleep(1);
}

export function handleSummary(data) {
  return {
    'stdout': textSummary(data, { indent: ' ', enableColors: true }),
    'stderr': jsonSummary(data),
  };
}

function textSummary(data, options) {
  const { metrics } = data;

  let summary = '\n';
  summary += '='.repeat(80) + '\n';
  summary += 'PERFORMANCE TEST SUMMARY\n';
  summary += '='.repeat(80) + '\n\n';

  summary += 'Response Time (P95):\n';
  summary += '-'.repeat(40) + '\n';

  const addMetric = (name, threshold, unit) => {
    const metric = metrics[name];
    if (metric) {
      const p95 = metric.values['p(95)'];
      const pass = p95 < threshold;
      summary += `${name}: ${p95.toFixed(2)}${unit} (threshold: ${threshold}${unit}) ${pass ? '✅' : '❌'}\n`;
    }
  };

  addMetric('login_duration', 500, 'ms');
  addMetric('create_room_duration', 800, 'ms');
  addMetric('send_message_duration', 600, 'ms');
  addMetric('sync_duration', 1000, 'ms');
  addMetric('room_summary_duration', 500, 'ms');

  summary += '\nError Rate:\n';
  summary += '-'.repeat(40) + '\n';
  summary += `errors: ${(metrics.errors.values.rate * 100).toFixed(2)}%\n`;

  summary += '\n';
  summary += '='.repeat(80) + '\n';

  return summary;
}

function jsonSummary(data) {
  return JSON.stringify({
    timestamp: new Date().toISOString(),
    metrics: data.metrics,
    thresholds_passed: checkThresholds(data.metrics),
  }, null, 2);
}

function checkThresholds(metrics) {
  const thresholds = {
    'login_duration': 500,
    'create_room_duration': 800,
    'send_message_duration': 600,
    'sync_duration': 1000,
    'room_summary_duration': 500,
  };

  const results = {};
  for (const [name, threshold] of Object.entries(thresholds)) {
    if (metrics[name]) {
      results[name] = {
        p95: metrics[name].values['p(95)'],
        threshold,
        passed: metrics[name].values['p(95)'] < threshold,
      };
    }
  }
  return results;
}
