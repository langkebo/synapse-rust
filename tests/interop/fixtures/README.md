# U-13 互操作 fixtures

这些文件是**本仓联邦流水线**（`synapse_common::pdu::build_pdu` →
`synapse_federation::event_finalize::finalize_local_pdu` →
`synapse_federation::signing::sign_and_hash_event`）在固定输入下产生的 PDU，
由 `tests/unit/u13_interop_fixture_tests.rs` 逐字节复核，因此不会与代码漂移。

用途：把"我们发出去的字节"交给**对等端自己的实现**复算，替代本沙箱做不到的
live `/send_join` + `/send`（原因见
`docs/audit/REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md` §5.2）。

```bash
# 用真实安装的 matrix-synapse（1.161.0）复算 hashes / event_id / signature
/tmp/peer-synapse/bin/python scripts/interop/verify_pdu_with_upstream_synapse.py \
    tests/interop/fixtures/local_pdu_v10.json
```

上游复算（任一不符即非零退出）：

1. `hashes.sha256` —— `synapse.crypto.event_signing.compute_content_hash`
2. 事件 ID —— `redact_event_dict` → canonical JSON → SHA-256 → unpadded Base64
   （v3 用标准字母表，v4+ 用 URL-safe，所以 v3 与 v10 的 ID 只差字母表）
3. 服务器签名 —— `signedjson.sign.verify_signed_json`
4. **v12 create 语义（MSC4291）**：`local_pdu_v12_create.json` 的 PDU 无 `room_id`
   （也无 `event_id`）→ 上游 `_check_create` **接受**并由事件 ID 推出同一个
   `!` + 43 字符 room ID；**带 `room_id` 的 create 必须被上游拒绝**（即使是"正确"的
   那个 room ID 也非法 —— 该字段本身就是循环）
5. **v12 `auth_events`（MSC4307）**：`auth_events` 里出现 create 事件的 PDU 必须被
   上游拒绝（上游从 `room_id` 反推 `$` + room_id[1:]）；同一次运行里先断言"未改动的
   PDU 能构建"，使该拒绝**可归因**，而不是"恰好构建失败"

**v10 与 v11 的 ID 不同是预期结果**：v11 的 redaction 不再保护顶层 `origin`，
redacted PDU 因此不同 ⇒ reference hash 不同。这正好证明签名/ID 走的是
**按房间版本**的 redaction。

**v12 的 room ID 是 domainless 的**（MSC4291）：`local_pdu_v12.json` 的 `pdu.room_id`
是 `!` + 43 字符 URL-safe base64、**没有 `:server` 部分**；
`local_pdu_v12_create.json` 则把"room ID 由 create 事件 ID 推导"钉成数据
（envelope 里的 `derived_room_id` = event_id 把 `$` 换成 `!`）。

`signing_seed_base64` 是测试用私钥种子（32 字节），**不是真实密钥**。

## 新增门禁的变红自证（AGENTS 铁律 8）

v12 的两条新检查都用**故意制造的违规**证明过会失败（2026-09-28）：

* **MSC4291 shape**：把 `room_id` 加进 create 并**重算 `hashes`、事件 ID 与签名**
  （这样第 1–3 步全过），oracle 仍以
  ``FAIL: a v12 m.room.create PDU must not carry `room_id` (MSC4291)`` 退出 1
  —— 证明该检查真的在拦人，而不是被上游的 hash/ID 检查"顺便"挡住。
* **derived_room_id**：把 envelope 的 `derived_room_id` 改成别的 43 字符串 ⇒
  ``FAIL: fixture records derived_room_id=… but `!` + event_id[1:] is …``，退出 1。
* **MSC4307**：把 `auth_events` 直接改成 create 事件 ID（写进 fixture）⇒ 第 5 步的
  **正向对照**（未改动 PDU 必须能构建）先失败，退出 1；而正常 fixture 下
  正向对照通过、构造出的变体被拒 —— 两侧都被执行到。

重新生成：

```bash
U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit -E 'test(/u13_interop_fixture/)'
```
