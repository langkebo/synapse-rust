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

上游复算的三件事（任一不符即非零退出）：

1. `hashes.sha256` —— `synapse.crypto.event_signing.compute_content_hash`
2. 事件 ID —— `redact_event_dict` → canonical JSON → SHA-256 → unpadded Base64
   （v3 用标准字母表，v4+ 用 URL-safe，所以 v3 与 v10 的 ID 只差字母表）
3. 服务器签名 —— `signedjson.sign.verify_signed_json`

**v10 与 v11 的 ID 不同是预期结果**：v11 的 redaction 不再保护顶层 `origin`，
redacted PDU 因此不同 ⇒ reference hash 不同。这正好证明签名/ID 走的是
**按房间版本**的 redaction。

`signing_seed_base64` 是测试用私钥种子（32 字节），**不是真实密钥**。

重新生成：

```bash
U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit -E 'test(/u13_interop_fixture/)'
```
