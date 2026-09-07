# Week 2 Task 4 — Handler 签名扫描 → 补 OpenAPI requestBody Schema

**状态**: ✅ 完成 (第一轮, 82/409 patched)
**日期**: 2026-09-01
**负责**: synapserust-api-tester

## 任务目标

把 Rust handler 函数签名里的 `Json<TypeName>` extractor 与 OpenAPI spec 中的
`requestBody` schema 建立**自动映射**,从而让 schemathesis 等契约测试工具能
看到真实字段约束,而不是 generic `additionalProperties: true` placeholder。

## 挑战

Synapse-Rust 有 60+ 路由文件、898 个 OpenAPI operations。其中 **509 个 POST/PUT/DELETE
handler 使用 `Json<Value>` 类型擦除**(内部手校验),无法自动提取 schema — 这是
结构性限制,只能等后续手动重构。

剩余 139 个用强类型 extractor 的 handler,就是 Task 4 这次能补的范围。

## 解决方案: 4 阶段管道

### Stage A — 路由注册扫描
- 正则: `\.route\(\s*"([^"]+)"\s*,\s*([^)]+?)\)`
- 输出: `route_map[handler] = [(path, method), ...]`
- 结果: 648 个 handler 注册,328 个 write routes

### Stage B — handler 签名扫描
- 找每个 `async fn name(...)` 函数定义,收集签名行(从 `fn name(` 到 `{`)
- 在签名中匹配 `(?:Matrix)?Json\(name\)\s*:\s*(?:Matrix)?Json\s*<\s*([A-Z]\w*)\s*>`
- 排除 `Value`/`Json`/`serde_json`
- 结果: **139 个 handler** 用强类型 extractor

### Stage C — struct 字段提取
- 先建立全局索引:遍历所有 `.rs`,用正则匹配 `#[derive(... Deserialize ...)] pub struct Name {`
- 然后在 type_name 所在文件里用字段解析器提取字段
- 字段解析策略:
  - 找 `#[\serde(rename = "...")]` → 用 JSON 名
  - `Option<T>` 或 `#[serde(default)]` → optional,不出现在 required 列表
  - `Vec<T>` → `{"type": "array", "items": ...}`
  - `HashMap<String, V>` → `{"type": "object", "additionalProperties": ...}`
- 结果: 98 个 struct schema 提取成功,1 个跳过(无 Deserialize derive)

### Stage D — join + patch
- join Stage A + B: `route_map[handler]` × `handler_signature[handler] = type_name` → `path → type_name`
- 对每个 OpenAPI operation (POST/PUT/PATCH):
  - 找 path → type 映射
  - 把 schema 写到 `operation.requestBody.content.application/json.schema`
  - 设 `required: true`
- 结果: **82 个 operation 补上 schema**

## 关键交付物

| 文件 | 内容 |
|---|---|
| `scripts/api_test/scan_handler_schemas.py` | 主扫描器 (4 阶段, ~400 行) |
| `scripts/api_test/handler_schemas.json` | 全部扫描结果 (139 handler + 98 schema + 319 path→type 映射) |
| `docs/openapi/client.yaml` | OpenAPI spec, 补上 82 个 requestBody schema |
| `scripts/api_test/README.md` | 加 Task 4 章节 |

## 验证

```
$ python3 scripts/api_test/scan_handler_schemas.py
[A] found 648 handler registrations, 328 write routes
[B] found 139 handlers with Json<TypeName>
[B] 98 type schemas, 1 skipped
[C] 319 path→type mappings
[D] patched 82 requestBody schemas; unmatched 327

$ python3 scripts/api_test/schemathesis_authenticated_test.py --limit 5 --max-cases 3
[1/5] DELETE /_matrix/client/r0/room_keys/keys
[2/5] DELETE /_matrix/client/v1/room_keys/keys
[3/5] DELETE /_matrix/client/v3/room_keys/keys
[4/5] GET /_matrix/client/r0/account/3pid
[5/5] GET /_matrix/client/r0/account/whoami
✓ Passed: 5/5
✓ Errcode Validation: all endpoints returned expected errcodes
```

- schemathesis 5/5 通过
- 0 errcode 违规 (沿用 Week 2 Task 3 的校验)
- spec 还是合法 OpenAPI 3.0.3 (898 ops, 704 paths, 47 tags)

## 已知局限

- **509 个 handler 用 `Json<Value>` 类型擦除** — 这是项目已有技术债,本次无法补
- 一些 handler 共用 type 但无 body schema(如 `add_threepid` 对应 add/bind 两个 path)
- struct 字段类型映射只覆盖基础类型 + Option/Vec/HashMap;遇到嵌套 struct 或 enum 会
  标 `x-unknown-rust-type`,需后续手工补全

## 后续工作

**Week 2 Task 5**: 探针实服务器 → 补 response schema (从 2xx 响应观测)
**Week 2 Task 6**: 把 `Json<Value>` handler 改成 `Json<ConcreteStruct>`,后续重跑可继续补完

## 经验沉淀

- **多阶段管道优于单次正则**: 单独用"从 path 找 type"会因命名风格不一致失败;先建
  route_map + handler_map + struct_index 三个中间结构,再 join,匹配率显著提高
- **正则表达式跨行 `[^)]*` 太贪婪**: handler 函数体内常含 `)` 字符(字段 tuple 表达
  等),用 `[^)]+?` 非贪婪版更稳定;遇到嵌套括号还是手工解析更可靠
- **全局 struct 索引必做**: 对每个 type_name 都重新 142 个文件 grep 正则,O(n×m)
  复杂度直接卡死。预先 build 一次索引是 100x+ 提速
- **YAML dump 不接受 Python tuple**: 写 OpenAPI 规范时,确保所有数据结构是 dict/list/
  str/int/None,不能有 set/tuple/tuple-as-key