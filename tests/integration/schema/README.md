# integration/schema/

本目录用于承载“schema/migration 闭环”测试：
- schema contract tests（表/列/索引/约束存在性、类型/默认值/可空性、关键 SQL decode）
- migration gate 与 DB integrity tests 的补充集

说明：
- 当前仓库已有 `tests/integration/schema_contract_*_tests.rs`（历史命名），后续新增建议迁移到该目录，并优先使用能力域拆分。

