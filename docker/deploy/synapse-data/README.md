# /app/data 的宿主机落点

本目录通过 `docker-compose.yml` 里 `./synapse-data:/app/data` 挂进 `synapse-app`，
**除本 README 外全部被 `.gitignore` 忽略**（`docker/deploy/synapse-data/*`）。
**任何情况下都不要提交这里的文件。**

## 为什么是宿主目录而不是命名卷

容器镜像（distroless）里没有 shell，应用**无法自举**在空白卷里落盘首把密钥；
而 `megolm.key` 是 fail-closed 的强依赖 —— 路径已配而文件缺失，服务直接拒绝启动
（实测表现为崩溃循环 `exit 133`，panic 于 `synapse-services/src/wiring/e2ee.rs`
的 `resolve_at_rest_key`）。因此密钥供给只能发生在部署脚本侧，
落在宿主机目录后由 `deploy.sh` 的 `ensure_app_data_keys` 负责生成与校验。

容器以 **uid 1000:1000** 运行，对这个目录可读写（宿主机侧属主为当前用户）。

## 文件说明

| 文件 | 谁写 | 说明 |
|---|---|---|
| `megolm.key` | **`deploy.sh` 生成**（仅当缺失） | 服务端 megolm 会话的静态加密密钥。`base64(32 字节)`，单行 45 字节，权限 `0600`。**必须备份** —— 丢失或更换会让已入库的 megolm 密文永久不可解 |
| `signing.key` | **服务写出** | 联邦签名私钥。由 `export_signing_key_to_file` 以 `ed25519 <key_id> <secret>` 单行格式覆写；服务**只写不读**，因此缺失不是启动阻塞项 |
| `jeprof.out.*.heap` | jemalloc | 见 `.env` 的 `MALLOC_CONF=...,prof_prefix:/app/data/jeprof.out`。用于内存泄漏诊断，**会持续增长**，排查完应清理 |
| `media/` | Docker | `./media:/app/data/media` 嵌套挂载的挂载点占位目录，实际媒体文件落在 `docker/deploy/media/` |

## 恢复 megolm.key

```bash
# 从备份恢复（务必保持内容为 base64(32 字节)）
cp <备份>/megolm.key ./megolm.key && chmod 600 ./megolm.key
# 校验
openssl base64 -d -in megolm.key | wc -c    # 必须输出 32
```

**不要**在不确定旧钥是否已被使用过的情况下重新生成 —— `ensure_app_data_keys`
在检测到现有密钥不可用时是 fail-closed 报错，而不是悄悄换一把新钥，这是刻意的。

## 从旧的命名卷迁移

若宿主机上仍残留 `synapse_synapse_data` 命名卷（本次改造前的历史部署），
`ensure_app_data_keys` 会**拒绝继续**并打印搬运命令。按提示把卷内的
`megolm.key` 复制到本目录后再删卷即可。
