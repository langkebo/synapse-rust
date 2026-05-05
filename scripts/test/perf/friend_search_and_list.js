import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8008';
const ADMIN_USER = __ENV.ADMIN_USER || 'admin';
const ADMIN_PASS = __ENV.ADMIN_PASS || 'Admin@123';
const SEARCH_TERM = __ENV.SEARCH_TERM || 'a';
const SEARCH_MODE = __ENV.SEARCH_MODE || 'fuzzy';
const SEARCH_LIMIT = Number(__ENV.SEARCH_LIMIT || '20');
const FRIEND_LIMIT = Number(__ENV.FRIEND_LIMIT || '20');

const errorRate = new Rate('errors');
const friendSearchDuration = new Trend('friend_search_duration');
const friendListDuration = new Trend('friend_list_duration');

export const options = {
  scenarios: {
    friends_directory: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '20s', target: 10 },
        { duration: '40s', target: 50 },
        { duration: '40s', target: 100 },
        { duration: '20s', target: 0 },
      ],
      gracefulRampDown: '10s',
    },
  },
  thresholds: {
    friend_search_duration: ['p(95)<400'],
    friend_list_duration: ['p(95)<300'],
    errors: ['rate<0.02'],
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
    'friend setup login ok': (r) => r.status === 200,
  });

  const body = loginRes.status === 200 ? JSON.parse(loginRes.body) : {};
  return {
    token: body.access_token || '',
  };
}

export default function (data) {
  const headers = {
    Authorization: `Bearer ${data.token}`,
    'Content-Type': 'application/json',
  };

  group('Friend Directory Search', () => {
    const res = http.get(
      `${BASE_URL}/_matrix/client/v3/friends/search?q=${encodeURIComponent(
        SEARCH_TERM
      )}&mode=${encodeURIComponent(SEARCH_MODE)}&limit=${SEARCH_LIMIT}`,
      { headers }
    );
    friendSearchDuration.add(res.timings.duration);

    const searchOk = check(res, {
      'friend search status 200': (r) => r.status === 200,
      'friend search has results': (r) => {
        const payload = JSON.parse(r.body || '{}');
        return Array.isArray(payload.results);
      },
    });
    errorRate.add(!searchOk);
  });

  group('Friend List Pagination', () => {
    const offset = (__ITER * FRIEND_LIMIT) % (FRIEND_LIMIT * 5);
    const res = http.get(
      `${BASE_URL}/_matrix/client/v3/friends?limit=${FRIEND_LIMIT}&offset=${offset}&sort_by=activity`,
      { headers }
    );
    friendListDuration.add(res.timings.duration);

    const listOk = check(res, {
      'friend list status 200': (r) => r.status === 200,
      'friend list has items array': (r) => {
        const payload = JSON.parse(r.body || '{}');
        return Array.isArray(payload.items);
      },
      'friend list has total': (r) => {
        const payload = JSON.parse(r.body || '{}');
        return typeof payload.total === 'number';
      },
    });
    errorRate.add(!listOk);
  });

  sleep(1);
}
