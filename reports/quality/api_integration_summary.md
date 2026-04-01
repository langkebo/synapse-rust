# API 集成测试结果摘要

- results_dir: `test-results`
- passed: `476`
- failed: `0`
- missing: `39`
- skipped: `39`

## 重点结论

- 无 FAILED：当前失败项为 0。
- 存在 MISSING：应视为后端缺口清单（端点缺失/未实现/不对齐），进入缺陷清单并排期补齐。

## Missing（Top Reasons）


## Skipped（Top Reasons）

- 25	requires federation signed request
- 8	destructive test
- 2	HTTP 404
- 1	federation signing key not configured

## 产物文件

- passed: `test-results/api-integration.passed.txt`
- failed: `test-results/api-integration.failed.txt`
- missing: `test-results/api-integration.missing.txt`
- skipped: `test-results/api-integration.skipped.txt`
