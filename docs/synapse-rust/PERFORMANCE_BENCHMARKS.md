# Performance Benchmark Configuration
# This file defines performance baselines and thresholds for API endpoints

## Benchmark Categories

### 1. Authentication Endpoints
| Endpoint | Method | Baseline (P50) | Threshold (P99) | Max |
|----------|--------|---------------|-----------------|-----|
| /_matrix/client/v3/login | POST | 50ms | 200ms | 500ms |
| /_matrix/client/v3/logout | POST | 10ms | 50ms | 100ms |
| /_matrix/client/v3/refresh | POST | 20ms | 100ms | 200ms |

### 2. Room Endpoints
| Endpoint | Method | Baseline (P50) | Threshold (P99) | Max |
|----------|--------|---------------|-----------------|-----|
| /_matrix/client/v3/createRoom | POST | 100ms | 500ms | 1000ms |
| /_matrix/client/v3/rooms/{id} | GET | 20ms | 100ms | 200ms |
| /_matrix/client/v3/rooms/{id}/join | POST | 50ms | 200ms | 500ms |
| /_matrix/client/v3/rooms/{id}/leave | POST | 50ms | 200ms | 500ms |

### 3. Sync Endpoints
| Endpoint | Method | Baseline (P50) | Threshold (P99) | Max |
|----------|--------|---------------|-----------------|-----|
| /_matrix/client/v3/sync | GET | 100ms | 500ms | 1000ms |
| /_matrix/client/v1/sync | GET | 100ms | 500ms | 1000ms |

### 4. Media Endpoints
| Endpoint | Method | Baseline (P50) | Threshold (P99) | Max |
|----------|--------|---------------|-----------------|-----|
| /_matrix/media/v3/upload | POST | 200ms | 1000ms | 2000ms |
| /_matrix/media/v3/download/{server}/{id} | GET | 50ms | 200ms | 500ms |
| /_matrix/media/v3/thumbnail/{server}/{id} | GET | 30ms | 150ms | 300ms |

### 5. Federation Endpoints
| Endpoint | Method | Baseline (P50) | Threshold (P99) | Max |
|----------|--------|---------------|-----------------|-----|
| /_matrix/federation/v1/version | GET | 10ms | 50ms | 100ms |
| /_matrix/federation/v1/state/{room} | GET | 50ms | 200ms | 500ms |
| /_matrix/federation/v1/publicRooms | GET | 100ms | 500ms | 1000ms |

## Throughput Baselines

| Category | Min TPS | Target TPS | Max TPS |
|----------|---------|------------|---------|
| Login | 10 | 50 | 100 |
| Sync | 50 | 200 | 500 |
| Room Operations | 20 | 100 | 200 |
| Media Upload | 5 | 20 | 50 |
| Federation | 10 | 50 | 100 |

## Error Rate Thresholds

| Category | Max Error Rate |
|----------|----------------|
| Authentication | 0.1% |
| Room Operations | 0.5% |
| Sync | 0.5% |
| Media | 1.0% |
| Federation | 1.0% |

## Memory Usage Baselines

| Endpoint Category | Baseline | Max |
|-------------------|----------|-----|
| Sync (with 1000 rooms) | 50MB | 100MB |
| Sync (with 10000 rooms) | 200MB | 500MB |
| Media Upload (50MB) | 100MB | 200MB |

## How to Use

1. **CI/CD Integration**: Add this to your CI pipeline to fail builds when thresholds are exceeded
2. **Local Testing**: Use `cargo bench` to measure current performance
3. **Production Monitoring**: Set up alerts when metrics exceed thresholds

## Example CI Check

```bash
#!/bin/bash
# Check performance benchmarks
cargo bench -- --noplot > bench.log
# Parse results and compare with baselines
# Exit with error if thresholds exceeded
```
