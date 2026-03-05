# Task 7.3 性能基准前后对比记录

- 基准对象：`cargo bench --bench database_bench`
- 前置基线：`--save-baseline task73_before`
- 对比执行：`--baseline task73_before`
- 原始输出：
  - `benches/results/task73_before.txt`
  - `benches/results/task73_after_compare.txt`

## 关键样本（ns/iter）

| Benchmark | Before | After | Change |
|---|---:|---:|---:|
| serialization/serialize_user | 257 | 244 | -5.06% |
| serialization/deserialize_user | 762 | 719 | -5.64% |
| strings/regex_match_multiple | 121 | 122 | +0.83% |
| data_structures/vec_search_1000 | 1445 | 1446 | +0.07% |
| collections/map_collect_100 | 4560 | 4588 | +0.61% |
| validation/matrix_id/valid | 53 | 52 | -1.89% |

## 结论

- 核心路径整体无明显性能回退。
- 主要指标波动处于基准测试噪声范围，可接受。
