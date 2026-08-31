# 04: Megolm 会话密钥双写改为直接 base64 编码

**What to build:** 双写路径要把 vodozemac 的会话密钥加密后写入 `session_key` 列。当前实现先把密文字节交给 JSON 序列化（得到 `[200,207,214,...]` 十进制数组），再对这段 ASCII 做一次 base64——等于二进制 → 十进制文本 → base64 双重冗余编码，实测把 60 字节放大到 280 字节（4.7 倍）。本票去掉中间的 JSON 环节，直接对密文字节做 base64。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

**审计条目：** #1（🟠 High）— Megolm session key 双写 `Vec<u8>` 走 JSON 数组序列化

**已核实的前置事实（决定了本票无兼容性风险）：**
- 注释中提到的兼容对象（legacy 加密入口）在当前代码库中已不存在，注释已过时
- 该列的加密产物在代码库中**没有任何解密读取方**：解密实际走 vodozemac pickle 副本
- 因此改编码不影响任何读取路径，无需数据迁移或双格式兼容

- [ ] 双写产物为 `base64(nonce ‖ ciphertext ‖ tag)` 原始字节，不再经过 JSON 序列化
- [ ] 编码结果可被同一 cipher 解密回原始会话密钥（往返可用，有单测锁定）
- [ ] 有单测断言编码后长度受控（不超过原始字节的常数倍），防止再次退回膨胀写法
- [ ] 双写关闭或缺密钥时的既有行为（返回 `None`、仅写 vodozemac 路径）保持不变
- [ ] `cargo test -p synapse-e2ee` 全绿，`cargo clippy -p synapse-e2ee -- -D warnings` 无告警
