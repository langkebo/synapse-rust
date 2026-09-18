#!/bin/bash
# =============================================================================
# container-migrate.sh — 部署编排用的薄包装（迁移唯一实现见 docker/db_migrate.sh）
# =============================================================================
# 本文件只做"容器内路径/环境 → 唯一实现所需环境"的桥接，然后 exec 真正实现。
#
# 历史上这里自带一份完整的迁移引擎（schema_migrations DDL、判断迁移是否已应用、
# 写入迁移记录、基线选择、历史基线跳过），与 docker/db_migrate.sh 构成同一职责的
# 第二份实现：内容校验和漂移检测一类修复必须写两遍，且部署路径可能停在一份过时
# 实现上（违反铁律 2）。现在所有 SQL/迁移逻辑只在 docker/db_migrate.sh。
#
# 保留本文件的原因：部署编排（docker-compose.yml 的 migrator 服务与 container_name）
# 与既有容器内路径 /scripts/container-migrate.sh 依赖它。
#
# 唯一实现查找顺序：
#   1. $MIGRATOR_SCRIPT 显式覆盖
#   2. 与本文件同级 —— 镜像内 /app/scripts/、migrator 容器 /scripts/
#   3. $REPO_ROOT/docker/db_migrate.sh —— 仓库内 <repo>/docker/deploy/scripts/ 直接运行
#
# 子命令（migrate / validate / status / init / help）原样透传，CLI 面不缩小。
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# 仓库内: <repo>/docker/deploy/scripts → <repo>；容器内: /scripts → /
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

MIGRATOR="${MIGRATOR_SCRIPT:-}"
if [ -z "$MIGRATOR" ]; then
    for candidate in "$SCRIPT_DIR/db_migrate.sh" "$REPO_ROOT/docker/db_migrate.sh"; do
        if [ -f "$candidate" ]; then
            MIGRATOR="$candidate"
            break
        fi
    done
fi
if [ -z "$MIGRATOR" ] || [ ! -f "$MIGRATOR" ]; then
    echo "[ERROR] 找不到迁移器实现 docker/db_migrate.sh" >&2
    echo "[ERROR] 容器内应挂载或内置到 $SCRIPT_DIR/db_migrate.sh；也可用 MIGRATOR_SCRIPT 指定。" >&2
    exit 2
fi

# 迁移目录：仓库内 <repo>/migrations；容器内即 canonical 挂载点 /migrations。
# 必须显式导出：唯一实现被挂载到 /scripts 时它自算的 PROJECT_ROOT 是 /，
# 而仓库内运行时它自算的是 <repo>/docker（没有 migrations 子目录）。
if [ -z "${MIGRATIONS_DIR:-}" ]; then
    candidate="${REPO_ROOT%/}/migrations" # REPO_ROOT=/ 时避免拼出 "//migrations"
    if [ -d "$candidate" ]; then
        MIGRATIONS_DIR="$candidate"
    else
        MIGRATIONS_DIR=/migrations
    fi
fi
export MIGRATIONS_DIR

# 容器内不存在"宿主 psql 打错实例"的 H-14 语境（DB_HOST 由 compose 给出，唯一实现
# 自身也会在容器内跳过该护栏）。这里只在确实位于容器内时默认放行，绝不在宿主上
# 直接运行本包装时关掉护栏 —— 那正是 H-14 要防的形态。
if [ -z "${SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL:-}" ]; then
    if [ -f /.dockerenv ] || grep -qaE 'docker|containerd|kubepods' /proc/1/cgroup 2>/dev/null; then
        export SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1
    fi
fi

exec bash "$MIGRATOR" "$@"
