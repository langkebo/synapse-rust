#!/bin/bash
# 配置验证脚本 - 检查所有必需的环境变量

set -e

echo "=== Synapse-Rust 配置验证 ==="
echo ""

# 必需的环境变量
REQUIRED_VARS=(
  "OLM_PICKLE_KEY"
  "SYNAPSE_DB_PASSWORD"
  "SYNAPSE_JWT_SECRET"
  "SYNAPSE_MACAROON_SECRET"
  "SYNAPSE_FORM_SECRET"
  "SYNAPSE_REGISTRATION_SECRET"
  "SYNAPSE_SECURITY_SECRET"
)

# 可选但推荐的环境变量
RECOMMENDED_VARS=(
  "SERVER_NAME"
  "DATABASE_URL"
  "REDIS_URL"
)

MISSING_REQUIRED=()
MISSING_RECOMMENDED=()

# 检查必需变量
for var in "${REQUIRED_VARS[@]}"; do
  if [ -z "${!var}" ]; then
    MISSING_REQUIRED+=("$var")
  else
    echo "✅ $var 已设置"
  fi
done

echo ""

# 检查推荐变量
for var in "${RECOMMENDED_VARS[@]}"; do
  if [ -z "${!var}" ]; then
    MISSING_RECOMMENDED+=("$var")
  else
    echo "✅ $var 已设置"
  fi
done

echo ""

# 报告结果
if [ ${#MISSING_REQUIRED[@]} -gt 0 ]; then
  echo "❌ 缺少必需的环境变量:"
  for var in "${MISSING_REQUIRED[@]}"; do
    echo "   - $var"
  done
  echo ""
  echo "请设置这些变量后再启动服务。"
  echo "参考: docs/ENVIRONMENT_VARIABLES.md"
  exit 1
fi

if [ ${#MISSING_RECOMMENDED[@]} -gt 0 ]; then
  echo "⚠️  缺少推荐的环境变量:"
  for var in "${MISSING_RECOMMENDED[@]}"; do
    echo "   - $var"
  done
  echo ""
  echo "这些变量使用默认值，建议在生产环境中显式设置。"
fi

echo ""
echo "✅ 配置验证通过！"
