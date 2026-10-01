#!/usr/bin/env python3
"""
Mock Data Generator for Synapse-Rust Monitoring
===============================================
用于在没有真实用户的情况下生成模拟监控数据

使用方法:
  python3 scripts/mock-data/generate_mock_data.py

功能:
  - 模拟用户登录/登出
  - 模拟消息发送
  - 模拟房间创建
  - 模拟 Sync 请求
  - 模拟 Federation 通信

生成的指标会直接写入 Prometheus 格式的文本文件，
Prometheus 可以配置为抓取这些数据。
"""

import os
import sys
import time
import random
import datetime
from pathlib import Path

# 输出目录
OUTPUT_DIR = Path(__file__).parent / "mock_metrics"
OUTPUT_DIR.mkdir(exist_ok=True)

def generate_timestamp():
    """生成当前时间戳"""
    return int(time.time())

def generate_room_operations():
    """生成房间操作指标"""
    operations = ['create', 'join', 'leave', 'upgrade', 'forget']
    outcomes = ['success', 'error', 'forbidden']
    versions = ['10', '11', '12']
    visibilities = ['public', 'private']
    
    lines = []
    lines.append("# HELP room_operations_total Total number of room operations")
    lines.append("# TYPE room_operations_total counter")
    
    for op in operations:
        for outcome in outcomes:
            for version in versions:
                for visibility in visibilities:
                    # 模拟不同操作的分布
                    count = random.randint(0, 100) if op == 'create' else random.randint(0, 500)
                    if op == 'join':
                        count += random.randint(100, 1000)
                    
                    labels = f'operation="{op}",outcome="{outcome}",room_version="{version}",visibility="{visibility}"'
                    lines.append(f'room_operations_total{{{labels}}} {count}')
    
    return '\n'.join(lines)

def generate_message_metrics():
    """生成消息投递指标"""
    stages = ['sent', 'persisted', 'sync_delivered', 'push_sent']
    room_types = ['public', 'private', 'space']
    msg_types = ['m.room.message', 'state', 'm.room.encryption']
    encryptions = ['true', 'false']
    
    lines = []
    lines.append("# HELP message_delivery_latency_seconds Message delivery latency")
    lines.append("# TYPE message_delivery_latency_seconds histogram")
    
    for stage in stages:
        for room_type in room_types:
            for msg_type in msg_types:
                for encrypted in encryptions:
                    # 模拟延迟分布（大部分快，少数慢）
                    base_latency = 0.1 if not encrypted else 0.3
                    
                    # 生成 bucket 计数
                    buckets = [
                        (0.05, int(random.uniform(80, 95))),
                        (0.1, int(random.uniform(150, 180))),
                        (0.25, int(random.uniform(250, 290))),
                        (0.5, int(random.uniform(350, 390))),
                        (1.0, int(random.uniform(450, 490))),
                        (2.5, int(random.uniform(490, 495))),
                        (float('+inf'), int(random.uniform(495, 500)))
                    ]
                    
                    cumulative = 0
                    for bucket, count in buckets:
                        cumulative += count
                        bucket_str = str(bucket) if bucket != float('+inf') else '+Inf'
                        labels = f'stage="{stage}",room_type="{room_type}",message_type="{msg_type}",encryption="{encrypted}",le="{bucket_str}"'
                        lines.append(f'message_delivery_latency_seconds_bucket{{{labels}}} {cumulative}')
                    
                    # 总和与计数
                    labels = f'stage="{stage}",room_type="{room_type}",message_type="{msg_type}",encryption="{encrypted}"'
                    total_time = base_latency * cumulative + random.uniform(10, 20)
                    lines.append(f'message_delivery_latency_seconds_sum{{{labels}}} {total_time:.3f}')
                    lines.append(f'message_delivery_latency_seconds_count{{{labels}}} {cumulative}')
    
    return '\n'.join(lines)

def generate_sync_metrics():
    """生成 Sync 延迟指标"""
    client_types = ['web', 'mobile', 'desktop']
    conn_types = ['polling', 'sse', 'websocket']
    room_sizes = ['small(<10)', 'medium(10-100)', 'large(>100)']
    
    lines = []
    lines.append("# HELP sync_event_delay_seconds Sync event delay")
    lines.append("# TYPE sync_event_delay_seconds histogram")
    
    for client in client_types:
        for conn in conn_types:
            for size in room_sizes:
                # 不同场景的延迟差异
                base_delay = {'web': 0.2, 'mobile': 0.3, 'desktop': 0.15}
                size_factor = {'small(<10)': 1, 'medium(10-100)': 2, 'large(>100)': 5}
                
                base = base_delay.get(client, 0.2) * size_factor.get(size, 1)
                
                # 生成延迟分布
                buckets = [
                    (0.1, int(random.uniform(50, 80))),
                    (0.25, int(random.uniform(150, 180))),
                    (0.5, int(random.uniform(250, 280))),
                    (1.0, int(random.uniform(350, 380))),
                    (2.0, int(random.uniform(390, 410))),
                    (5.0, int(random.uniform(395, 415))),
                    (float('+inf'), int(random.uniform(410, 420)))
                ]
                
                cumulative = 0
                for bucket, count in buckets:
                    cumulative += count
                    bucket_str = str(bucket) if bucket != float('+inf') else '+Inf'
                    labels = f'client_type="{client}",connection_type="{conn}",room_size="{size}",le="{bucket_str}"'
                    lines.append(f'sync_event_delay_seconds_bucket{{{labels}}} {cumulative}')
                
                labels = f'client_type="{client}",connection_type="{conn}",room_size="{size}"'
                total_delay = base * cumulative + random.uniform(5, 15)
                lines.append(f'sync_event_delay_seconds_sum{{{labels}}} {total_delay:.3f}')
                lines.append(f'sync_event_delay_seconds_count{{{labels}}} {cumulative}')
    
    return '\n'.join(lines)

def generate_e2ee_metrics():
    """生成 E2EE 密钥交换指标"""
    algorithms = ['olm.v1.curve25519-aes-sha2', 'megolm.v1.aes-sha2']
    operations = ['session_creation', 'key_share', 'key_request']
    device_counts = ['1', '2-5', '6-10', '10+']
    
    lines = []
    lines.append("# HELP e2ee_handshake_duration_seconds E2EE handshake duration")
    lines.append("# TYPE e2ee_handshake_duration_seconds histogram")
    
    for algo in algorithms:
        for op in operations:
            for device_count in device_counts:
                # 设备越多越慢
                device_factor = {'1': 1, '2-5': 2, '6-10': 4, '10+': 8}
                factor = device_factor.get(device_count, 1)
                
                base_duration = 0.5 * factor
                
                buckets = [
                    (0.1, int(random.uniform(20, 40))),
                    (0.25, int(random.uniform(80, 120))),
                    (0.5, int(random.uniform(180, 220))),
                    (1.0, int(random.uniform(280, 320))),
                    (2.0, int(random.uniform(350, 380))),
                    (5.0, int(random.uniform(390, 410))),
                    (float('+inf'), int(random.uniform(410, 420)))
                ]
                
                cumulative = 0
                for bucket, count in buckets:
                    cumulative += count
                    bucket_str = str(bucket) if bucket != float('+inf') else '+Inf'
                    labels = f'algo="{algo}",operation="{op}",device_count="{device_count}",le="{bucket_str}"'
                    lines.append(f'e2ee_handshake_duration_seconds_bucket{{{labels}}} {cumulative}')
                
                labels = f'algo="{algo}",operation="{op}",device_count="{device_count}"'
                total_duration = base_duration * cumulative + random.uniform(10, 30)
                lines.append(f'e2ee_handshake_duration_seconds_sum{{{labels}}} {total_duration:.3f}')
                lines.append(f'e2ee_handshake_duration_seconds_count{{{labels}}} {cumulative}')
    
    return '\n'.join(lines)

def generate_business_metrics():
    """生成业务指标"""
    lines = []
    
    # 活跃用户数
    lines.append("# HELP auth_success Total successful authentications")
    lines.append("# TYPE auth_success counter")
    lines.append(f'auth_success {{instance="synapse-rust-main",env="production"}} {random.randint(10000, 15000)}')
    
    # 房间总数
    lines.append("# HELP room_count Total number of rooms")
    lines.append("# TYPE room_count gauge")
    lines.append(f'room_count {{instance="synapse-rust-main"}} {random.randint(5000, 8000)}')
    
    # E2EE 覆盖率
    lines.append("# HELP e2ee_enabled_rate Rate of encrypted rooms")
    lines.append("# TYPE e2ee_enabled_rate gauge")
    lines.append(f'e2ee_enabled_rate {{instance="synapse-rust-main"}} {random.uniform(0.75, 0.85):.2f}')
    
    # 消息发送量
    lines.append("# HELP messages_sent_total Total messages sent")
    lines.append("# TYPE messages_sent_total counter")
    lines.append(f'messages_sent_total {{encrypted="true"}} {random.randint(100000, 150000)}')
    lines.append(f'messages_sent_total {{encrypted="false"}} {random.randint(50000, 80000)}')
    
    return '\n'.join(lines)

def generate_database_metrics():
    """生成数据库和缓存指标"""
    lines = []
    
    # 数据库查询
    lines.append("# HELP db_queries_total Total database queries")
    lines.append("# TYPE db_queries_total counter")
    tables = ['rooms', 'events', 'members', 'devices']
    operations = ['SELECT', 'INSERT', 'UPDATE', 'DELETE']
    
    for table in tables:
        for op in operations:
            count = random.randint(1000, 10000)
            lines.append(f'db_queries_total {{table="{table}",operation="{op}"}} {count}')
    
    # 数据库连接池
    lines.append("# HELP db_connection_pool_utilization Connection pool utilization")
    lines.append("# TYPE db_connection_pool_utilization gauge")
    lines.append(f'db_connection_pool_utilization {{pool="main"}} {random.uniform(0.3, 0.7):.2f}')
    
    # 缓存
    lines.append("# HELP cache_operations_total Total cache operations")
    lines.append("# TYPE cache_operations_total counter")
    cache_types = ['room_state', 'event_body', 'membership']
    operations = ['get', 'set', 'delete', 'invalidate']
    
    for cache_type in cache_types:
        for op in operations:
            count = random.randint(5000, 20000)
            lines.append(f'cache_operations_total {{cache_type="{cache_type}",operation="{op}"}} {count}')
    
    # 缓存命中率
    lines.append("# HELP cache_hit_rate Cache hit rate")
    lines.append("# TYPE cache_hit_rate gauge")
    lines.append(f'cache_hit_rate {{cache_type="room_state"}} {random.uniform(0.85, 0.95):.2f}')
    lines.append(f'cache_hit_rate {{cache_type="event_body"}} {random.uniform(0.80, 0.90):.2f}')
    lines.append(f'cache_hit_rate {{cache_type="membership"}} {random.uniform(0.88, 0.93):.2f}')
    
    return '\n'.join(lines)

def main():
    """主函数：生成所有模拟数据"""
    print("🚀 开始生成模拟监控数据...")
    print()
    
    # 生成时间戳
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"⏰ 生成时间：{timestamp}")
    print()
    
    # 生成各部分指标
    print("📊 生成房间操作指标...")
    room_ops = generate_room_operations()
    
    print("💬 生成消息投递指标...")
    messages = generate_message_metrics()
    
    print("🔄 生成 Sync 延迟指标...")
    sync = generate_sync_metrics()
    
    print("🔐 生成 E2EE 指标...")
    e2ee = generate_e2ee_metrics()
    
    print("👥 生成业务指标...")
    business = generate_business_metrics()
    
    print("🗄️ 生成数据库/缓存指标...")
    db_cache = generate_database_metrics()
    
    # 写入文件
    output_file = OUTPUT_DIR / "mock_synapse_metrics.prom"
    with open(output_file, 'w') as f:
        f.write(f'# Mock Synapse-Rust Metrics - Generated at {timestamp}\n\n')
        f.write(room_ops + '\n\n')
        f.write(messages + '\n\n')
        f.write(sync + '\n\n')
        f.write(e2ee + '\n\n')
        f.write(business + '\n\n')
        f.write(db_cache + '\n')
    
    print()
    print(f"✅ 模拟数据已生成!")
    print(f"📁 输出文件：{output_file.absolute()}")
    print()
    print("📋 下一步:")
    print("   1. 在 Prometheus 配置中添加文件抓取器:")
    print("      - job_name: 'mock-synapse'")
    print("        static_configs:")
    print("          - targets: ['file://path/to/mock_synapse_metrics.prom']")
    print()
    print("   2. 或使用 pushgateway 推送数据:")
    print("      curl -X POST --data-binary @mock_synapse_metrics.prom")
    print("      http://localhost:9091/metrics/job/mock-synapse")
    print()
    print("   3. 最简单：直接查看生成的文件内容")
    print(f"      cat {output_file.absolute()}")

if __name__ == '__main__':
    main()
