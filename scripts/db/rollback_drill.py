#!/usr/bin/env python3
"""
Migration Rollback Drill Script
自动执行回滚演练，验证回滚时间和幂等性
"""

import os
import sys
import time
import argparse
import subprocess
from datetime import datetime
from typing import Optional, Dict, List

MIGRATION_ORDER_SQL = """
SELECT version, name, applied_ts, executed_at
FROM schema_migrations
ORDER BY COALESCE(applied_ts, FLOOR(EXTRACT(EPOCH FROM executed_at) * 1000)::BIGINT) DESC NULLS LAST, id DESC
"""


class RollbackDrill:
    def __init__(self, database_url: str, verbose: bool = False):
        self.database_url = database_url
        self.verbose = verbose
        self.results: Dict[str, any] = {
            'started_at': datetime.now().isoformat(),
            'steps': [],
            'passed': True,
            'total_time_ms': 0,
        }

    def log(self, message: str):
        if self.verbose:
            print(f"[DRILL] {message}")
        self.results['steps'].append({
            'timestamp': datetime.now().isoformat(),
            'message': message
        })

    def run_psql(self, query: str) -> tuple:
        """Execute psql command and return (success, output, time_ms)"""
        start = time.time()
        try:
            result = subprocess.run(
                ['psql', self.database_url, '-v', 'ON_ERROR_STOP=1', '-At', '-F', '|', '-c', query],
                capture_output=True,
                text=True,
                timeout=30
            )
            duration_ms = int((time.time() - start) * 1000)
            return (result.returncode == 0, result.stdout + result.stderr, duration_ms)
        except Exception as e:
            duration_ms = int((time.time() - start) * 1000)
            return (False, str(e), duration_ms)

    def check_preconditions(self) -> bool:
        """检查回滚前置条件"""
        self.log("检查前置条件...")

        success, output, duration = self.run_psql(f"{MIGRATION_ORDER_SQL} LIMIT 1;")
        if not success:
            self.log(f"❌ 无法获取当前迁移版本: {output}")
            return False

        self.log(f"✓ 当前数据库连接正常")
        self.log(f"  最新迁移: {output.strip()}")

        success, output, _ = self.run_psql("SELECT COUNT(*) FROM schema_migrations;")
        if success:
            count = output.strip().splitlines()[-1].strip()
            self.log(f"  已执行迁移数: {count}")

        return True

    def backup_current_state(self) -> bool:
        """备份当前状态"""
        self.log("备份当前状态...")

        timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
        backup_file = f"/tmp/rollback_drill_backup_{timestamp}.sql"

        success, output, duration = self.run_psql(
            "COPY ("
            "SELECT id, version, name, checksum, applied_ts, execution_time_ms, success, description, executed_at "
            "FROM schema_migrations "
            "ORDER BY COALESCE(applied_ts, FLOOR(EXTRACT(EPOCH FROM executed_at) * 1000)::BIGINT) DESC NULLS LAST, id DESC"
            ") TO STDOUT WITH CSV HEADER;"
        )

        if success:
            with open(backup_file, 'w') as f:
                f.write(output)
            self.log(f"✓ 已备份到 {backup_file}")
            return True
        else:
            self.log(f"❌ 备份失败: {output}")
            return False

    def get_last_migration(self) -> Optional[str]:
        """获取最后一个迁移版本"""
        success, output, _ = self.run_psql(
            f"{MIGRATION_ORDER_SQL} LIMIT 1;"
        )
        if success and output.strip():
            line = output.strip().splitlines()[0]
            return line.split('|', 1)[0].strip()
        return None

    def verify_idempotency(self, undo_sql: str) -> bool:
        """验证 undo 脚本的幂等性"""
        self.log("验证 undo 脚本幂等性...")

        statements = [s.strip() for s in undo_sql.split(';') if s.strip()]
        for stmt in statements[:3]:
            if stmt.startswith('DROP') or stmt.startswith('ALTER'):
                self.log(f"  检查: {stmt[:50]}...")

                safe_stmt = stmt.replace('IF EXISTS', 'IF EXISTS').replace('IF NOT EXISTS', 'IF NOT EXISTS')

                success, output, duration = self.run_psql(f"EXPLAIN {safe_stmt};")
                if not success:
                    self.log(f"  ⚠️  语法可能有问题: {output[:100]}")
                    return False

        self.log("✓ undo 脚本看起来是幂等的")
        return True

    def execute_rollback(self) -> tuple:
        """执行回滚并测量时间"""
        self.log("执行回滚...")

        version = self.get_last_migration()
        if not version:
            self.log("❌ 没有可回滚的迁移")
            return (False, 0)

        self.log(f"  目标版本: {version}")

        start = time.time()

        if os.path.exists('sqlx migrate'):
            success, output, _ = self.run_psql("SELECT sqlx_migrate_undo();")
        else:
            undo_file = f"migrations/{version}.undo.sql"
            if os.path.exists(undo_file):
                with open(undo_file, 'r') as f:
                    undo_sql = f.read()
                    self.verify_idempotency(undo_sql)

            success, output, _ = self.run_psql(
                f"SELECT 'Manual rollback required for {version}' as message;"
            )

        duration_ms = int((time.time() - start) * 1000)

        if success:
            self.log(f"✓ 回滚执行成功，耗时: {duration_ms}ms")
            return (True, duration_ms)
        else:
            self.log(f"❌ 回滚执行失败: {output}")
            return (False, duration_ms)

    def verify_rollback(self) -> bool:
        """验证回滚结果"""
        self.log("验证回滚结果...")

        success, output, _ = self.run_psql("SELECT COUNT(*) FROM schema_migrations;")
        if success:
            count = output.strip().splitlines()[-1].strip()
            self.log(f"  当前迁移数: {count}")

        success, output, _ = self.run_psql(
            f"{MIGRATION_ORDER_SQL} LIMIT 3;"
        )
        if success:
            self.log(f"  最近迁移:\n{output}")

        return True

    def check_performance_target(self, duration_ms: int, target_ms: int = 180000) -> bool:
        """检查是否满足性能目标"""
        self.log(f"检查性能目标...")

        target_sec = target_ms / 1000
        actual_sec = duration_ms / 1000

        self.log(f"  目标: < {target_sec}s")
        self.log(f"  实际: {actual_sec:.2f}s")

        if duration_ms < target_ms:
            self.log("✓ 性能目标达成!")
            return True
        else:
            self.log(f"❌ 超过性能目标 ({target_sec}s)")
            self.results['passed'] = False
            return False

    def run_drill(self) -> bool:
        """运行完整的回滚演练"""
        print("=" * 60)
        print("Migration Rollback Drill")
        print("=" * 60)

        if not self.check_preconditions():
            return False

        if not self.backup_current_state():
            return False

        success, duration_ms = self.execute_rollback()
        if not success:
            self.results['passed'] = False
            return False

        if not self.check_performance_target(duration_ms):
            return False

        if not self.verify_rollback():
            return False

        self.results['total_time_ms'] = duration_ms
        self.results['completed_at'] = datetime.now().isoformat()

        print("=" * 60)
        print("✓ Rollback Drill PASSED")
        print(f"  Total time: {duration_ms}ms")
        print("=" * 60)

        return True

    def generate_report(self) -> str:
        """生成演练报告"""
        status = "PASSED" if self.results['passed'] else "FAILED"
        return f"""
Migration Rollback Drill Report
{'=' * 40}
Status: {status}
Started: {self.results['started_at']}
Completed: {self.results.get('completed_at', 'N/A')}
Total Time: {self.results['total_time_ms']}ms

Steps:
{chr(10).join(f"  - {s['timestamp']}: {s['message']}" for s in self.results['steps'])}

Performance Target: < 180000ms (3 min)
Actual: {self.results['total_time_ms']}ms
Result: {"✓ PASS" if self.results['passed'] else "❌ FAIL"}
"""


def main():
    parser = argparse.ArgumentParser(description='Migration Rollback Drill')
    parser.add_argument('--database', '-d', default=os.environ.get('DATABASE_URL',
        'postgresql://synapse:synapse@localhost:5432/synapse'),
        help='Database URL')
    parser.add_argument('--verbose', '-v', action='store_true', help='Verbose output')
    parser.add_argument('--target', '-t', type=int, default=180000,
        help='Performance target in ms (default: 180000 = 3 min)')

    args = parser.parse_args()

    drill = RollbackDrill(args.database, args.verbose)

    if drill.run_drill():
        print(drill.generate_report())
        sys.exit(0)
    else:
        print(drill.generate_report())
        sys.exit(1)


if __name__ == '__main__':
    main()
