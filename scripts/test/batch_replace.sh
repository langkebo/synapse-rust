#!/bin/bash
# 批量替换脚本 - 将 curl+grep 模式转换为 http_json+check_success_json
# 备份已创建: api-integration_test.sh.bak.*

BACKUP_FILE="/Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh"
TARGET_FILE="/Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh"

echo "=== 批量替换 curl+grep 模式为 http_json+check_success_json ==="
echo "使用备份: $BACKUP_FILE"
echo ""

# 使用 perl 进行批量替换（更强大的正则表达式支持）

# 模式1: curl -s "$SERVER_URL/..." -H "Authorization: Bearer $TOKEN" | grep -q "keyword" && pass "Name" || fail
perl -i -pe '
  s{curl -s\s+"(https?://\$\{?SERVER_URL\}?[^"]+)"\s+-H\s+"Authorization:\s+Bearer\s+\$\{?TOKEN\}?"\s+\|\s+grep\s+-q\s+"([^"]+)"\s+&&\s+pass\s+"([^"]+)"\s+\|\|\s+fail\s+"([^"]+)";}{
    my $url = $1;
    my $keyword = $2;
    my $pass_name = $3;
    $keyword =~ s/\\\|/|/g;
    "http_json GET \"$url\" \"\$TOKEN\"\ncheck_success_json \"\$HTTP_BODY\" \"\$HTTP_STATUS\" \"$keyword\" \&\& pass \"$pass_name\" || fail \"$pass_name\"";
  }ge;
' "$TARGET_FILE"

echo "模式1替换完成 (TOKEN)"

# 模式2: curl -s "$SERVER_URL/..." -H "Authorization: Bearer $ADMIN_TOKEN" | grep -q "keyword" && pass "Name" || fail
perl -i -pe '
  s{curl -s\s+"(https?://\$\{?SERVER_URL\}?[^"]+)"\s+-H\s+"Authorization:\s+Bearer\s+\$\{?ADMIN_TOKEN\}?"\s+\|\s+grep\s+-q\s+"([^"]+)"\s+&&\s+pass\s+"([^"]+)"\s+\|\|\s+fail\s+"([^"]+)";}{
    my $url = $1;
    my $keyword = $2;
    my $pass_name = $3;
    $keyword =~ s/\\\|/|/g;
    "http_json GET \"$url\" \"\$ADMIN_TOKEN\"\ncheck_success_json \"\$HTTP_BODY\" \"\$HTTP_STATUS\" \"$keyword\" \&\& pass \"$pass_name\" || fail \"$pass_name\"";
  }ge;
' "$TARGET_FILE"

echo "模式2替换完成 (ADMIN_TOKEN)"

# 模式3: curl -s "$SERVER_URL/..." | grep -q "keyword" && pass "Name" || fail (无认证)
perl -i -pe '
  s{curl -s\s+"(https?://\$\{?SERVER_URL\}?[^"]+)"\s+\|\s+grep\s+-q\s+"([^"]+)"\s+&&\s+pass\s+"([^"]+)"\s+\|\|\s+fail\s+"([^"]+)";}{
    my $url = $1;
    my $keyword = $2;
    my $pass_name = $3;
    $keyword =~ s/\\\|/|/g;
    "http_json GET \"$url\" \"\"\ncheck_success_json \"\$HTTP_BODY\" \"\$HTTP_STATUS\" \"$keyword\" \&\& pass \"$pass_name\" || fail \"$pass_name\"";
  }ge;
' "$TARGET_FILE"

echo "模式3替换完成 (无认证)"

# 模式4: curl -s "$SERVER_URL/..." | grep -q "keyword" && pass "Name" || skip
perl -i -pe '
  s{curl -s\s+"(https?://\$\{?SERVER_URL\}?[^"]+)"\s+\|\s+grep\s+-q\s+"([^"]+)"\s+&&\s+pass\s+"([^"]+)"\s+\|\|\s+skip\s+"([^"]+)";}{
    my $url = $1;
    my $keyword = $2;
    my $pass_name = $3;
    my $skip_reason = $4;
    $keyword =~ s/\\\|/|/g;
    "http_json GET \"$url\" \"\"\ncheck_success_json \"\$HTTP_BODY\" \"\$HTTP_STATUS\" \"$keyword\" \&\& pass \"$pass_name\" || skip \"$pass_name\" \"\$ASSERT_ERROR\"";
  }ge;
' "$TARGET_FILE"

echo "模式4替换完成 (skip无认证)"

# 模式5: curl -s "$SERVER_URL/..." -H "Authorization: Bearer $TOKEN" | grep -q "keyword" && pass "Name" || skip
perl -i -pe '
  s{curl -s\s+"(https?://\$\{?SERVER_URL\}?[^"]+)"\s+-H\s+"Authorization:\s+Bearer\s+\$\{?TOKEN\}?"\s+\|\s+grep\s+-q\s+"([^"]+)"\s+&&\s+pass\s+"([^"]+)"\s+\|\|\s+skip\s+"([^"]+)";}{
    my $url = $1;
    my $keyword = $2;
    my $pass_name = $3;
    $keyword =~ s/\\\|/|/g;
    "http_json GET \"$url\" \"\$TOKEN\"\ncheck_success_json \"\$HTTP_BODY\" \"\$HTTP_STATUS\" \"$keyword\" \&\& pass \"$pass_name\" || skip \"$pass_name\" \"\$ASSERT_ERROR\"";
  }ge;
' "$TARGET_FILE"

echo "模式5替换完成 (TOKEN skip)"

echo ""
echo "=== 批量替换完成 ==="
echo "验证语法: bash -n $TARGET_FILE"
