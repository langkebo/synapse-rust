# 02: AES-GCM 改用 96 位全随机 nonce

**What to build:** AES-GCM 的 nonce 目前由「4 字节随机前缀 + 8 字节计数器」构成，计数器从 0 起。服务进程每次重建（重启、Pod 重建）计数器都会归零，而加密密钥是从配置读取的**持久密钥**——跨重启后 nonce 唯一性只剩 32 位随机前缀，在反复重启的场景下存在真实的重用概率。同密钥重用 nonce 对 GCM 是灾难性的（泄露认证子密钥）。本票把 AES-GCM 的 nonce 换成符合 NIST SP 800-38D 的 96 位全随机方案，彻底摆脱对跨进程持久状态的依赖。

**Blocked by:** 01（追踪器重构：本票改的是 nonce 构造方式，登记/检查仍走重构后的追踪器，先改好追踪器可避免二次返工）

**Status:** done — pre-implemented by commit `f09304e5` on 2026-09-01 (5 days before this audit ticket was filed)

**审计条目：** #2（🟠 High）— AES-GCM nonce 计数器在实例重建时归零

## 验收（2026-09-07）

**Pre-existing 实现（commit `f09304e5` 2026-09-01）**：
- `f09304e5`: `fix(e2ee): draw AES-GCM nonces fully at random (96-bit), drop the counter`
- AES-GCM 12 字节 nonce 全随机，不再用 4 字节前缀 + 8 字节计数器
- XChaCha20Poly1305 24 字节 nonce 保持不变
- ticket 的"被 01 阻塞"已失效：01 nonce tracker 早已支持 12/24 字节双长度，`f09304e5` 无需依赖 NonceTracker 重构

- [ ] AES-GCM nonce 的 12 字节全部由 CSPRNG 填充，不再包含进程内计数器
- [ ] 进程重启 / 服务实例重建后，nonce 唯一性不依赖任何跨进程的持久状态
- [ ] 计数器不再参与 nonce 构造，但保留其可观测性用途（累计生成数、溢出保护语义需重新定义或移除）
- [ ] 移除计数器后若 `NonceCounterOverflow` 不再可达，需确认错误变体的去留不破坏既有错误处理分支
- [ ] 24 字节 nonce 的对应路径（仅测试使用）保持可用，不被本票破坏
- [ ] `cargo test -p synapse-e2ee` 全绿，`cargo clippy -p synapse-e2ee -- -D warnings` 无告警
