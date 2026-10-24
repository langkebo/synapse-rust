//! 联邦设备列表 EDU 的**写入方式**不许退回"外来序号 + 影子表"（审计 NEW-07）。
//!
//! # 为什么行为用例不够
//!
//! `tests/integration/device_list_federation_visibility_tests.rs` 证明的是
//! `AccountDeviceListService::record_device_list_change` 写出来的东西在 `/sync` 与
//! `/keys/changes` 的读取面上可见。它**证明不了** `federation::edu` 的两个 handler
//! 真的调这一个方法 —— 只要有人把 handler 改回
//! `insert_device_list_change(.., 远端 stream_id)`（那个方法已被删除，所以下次会换个名字
//! 重新长出来），行为用例照样绿，因为它测的是服务层。
//!
//! 这条守卫补的正是这一段：**入站 handler 只允许调用存储层的本地序号写入**。
//! 判据是源码级的三条：
//!
//! 1. `federation/edu.rs` 里 `record_device_list_change` 至少出现两次
//!    （`m.device_list_update` 与 `m.profile_update` 各一次）；
//! 2. 全仓不存在 `insert_device_list_change`（"我来给 stream_id" 的那条老路）；
//! 3. 两个 handler 的函数体里不出现 `current_timestamp_millis()`（墙上时钟当序号）。

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// 取一个 `async fn` 的函数体（从签名到下一个独占一行的 `}`）。
fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source.find(signature).unwrap_or_else(|| panic!("源码里找不到 `{signature}`"));
    let rest = &source[start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |i| i + 2);
    &rest[..end]
}

/// 入站 handler 必须用存储层分配的本地序号。
#[test]
fn inbound_device_list_edus_use_the_local_stream_id_writer() {
    const EDU: &str = "synapse-web/src/federation/edu.rs";
    let src = read(EDU);

    let calls = src.matches("record_device_list_change").count();
    assert!(
        calls >= 2,
        "`{EDU}` 里 `record_device_list_change` 只出现 {calls} 次。两个入站路径都必须用它：\n\
         · `handle_device_list_update_edu`（远端设备增删改）\n\
         · `m.profile_update` 的兜底分支（用户级 profile 变更也要 bump 设备列表流）\n\
         少一处就意味着那条路径又变成了\"写了对客户端不可见的东西\"。"
    );

    let body = function_body(&src, "async fn handle_device_list_update_edu(");
    assert!(
        body.contains("record_device_list_change"),
        "`handle_device_list_update_edu` 必须调用 `record_device_list_change`（由存储层分配本地序号）；\n\
         实际函数体：\n{body}"
    );
}

/// 全仓不许再出现"调用方给 stream_id"的写入路径。
///
/// 这是 NEW-07 的**根因**：一旦允许调用方传 `stream_id`，联邦那条调用就会把对端的
/// 毫秒时间戳（1e12）塞进本地序列（1e3），于是那一行落在所有 `stream_id <= to` 的读取
/// 之外 —— 而且不会有任何报错。
///
/// **只扫生产目录，且剥掉整行注释**：第一版直接全仓 `contains`，结果命中三处——
/// 本文件自己、集成用例的说明、以及 `edu.rs` 里那句"以前这里调的是 …"的注释。
/// 门禁读注释就会把"记录缺陷"判成"缺陷还在"，这是本仓反复踩过的坑
/// （`user_flow_tests_have_an_executable_dispatch_lane` 也被同一个坑骗过两次）。
#[test]
fn nothing_writes_device_list_changes_with_a_foreign_stream_id() {
    const ROOTS: [&str; 7] = [
        "synapse-cache/src",
        "synapse-common/src",
        "synapse-e2ee/src",
        "synapse-federation/src",
        "synapse-services/src",
        "synapse-storage/src",
        "synapse-web/src",
    ];

    let mut offenders = Vec::new();
    for root in ROOTS {
        let mut stack = vec![repo_root().join(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let text = fs::read_to_string(&path).unwrap_or_default();
                let live: String =
                    text.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
                if live.contains("insert_device_list_change") {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`insert_device_list_change`（调用方自带 `stream_id`）必须保持不存在 —— 它就是 NEW-07：\n\
         联邦 EDU 带来的对端毫秒时间戳会被写进本地序列，那一行随后对所有读取都不可见。\n\
         需要记一条设备列表变更时用 `record_device_list_change`（序号由存储层分配）。\n\
         命中文件：{offenders:?}"
    );
}

/// 两个 handler 里不许把墙上时钟当流序号。
#[test]
fn device_list_edu_handlers_do_not_use_the_wall_clock_as_a_stream_id() {
    const EDU: &str = "synapse-web/src/federation/edu.rs";
    let src = read(EDU);

    for signature in ["async fn handle_device_list_update_edu(", "async fn handle_profile_update_edu("] {
        assert!(src.contains(signature), "`{signature}` 不见了：本守卫需要随函数改名更新");
        let body = function_body(&src, signature);
        assert!(
            !body.contains("let stream_id = current_timestamp_millis()"),
            "`{signature}` 里还在用墙上时钟当设备列表流序号。\n\
             毫秒时间戳（1e12 量级）与本地序列（1e3 量级）不可比：所有读取都带\n\
             `stream_id <= to`，写进去等于把这行排除在所有读取之外（NEW-07）。\n\
             实际函数体：\n{body}"
        );
    }
}

/// 那条"消费掉但不做事"的死作业不许复活。
///
/// 背景（NEW-06 的更正）：`device_sync::notify_device_revocation` **没有任何调用者**，
/// 它入队的 `Generic{notify_device_revocation}` 也从未产生过；真正的设备列表 EDU 广播在
/// `synapse-web/src/routes/device.rs::broadcast_device_list_update`（签名事务，
/// `EventBroadcaster::broadcast_edu`）。留着那条路径的代价不是"多几行"，而是**两份实现**
/// ——其中一份未签名、也永远不会被调用，却挂在公开 API 上。
#[test]
fn the_dead_device_revocation_job_does_not_come_back() {
    let mut hits = Vec::new();
    for rel in
        ["synapse-federation/src/device_sync.rs", "src/bin/synapse_worker.rs", "synapse-common/src/background_job.rs"]
    {
        // 剥注释（与上一条同一理由）：文件里写"这里删过什么"是正当的记录，
        // 门禁只该盯**代码**。
        let live: String =
            read(rel).lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        if live.contains("notify_device_revocation") || live.contains("BackgroundJob::Generic") {
            hits.push(rel);
        }
    }
    assert!(
        hits.is_empty(),
        "已删除的死路径又回来了：{hits:?}\n\
         · 设备列表 EDU 的真实广播在 `routes/device.rs::broadcast_device_list_update`；\n\
         · 若确实需要新的后台作业，就给它一个具名变体并写进 `WorkerSupport` 分类，\n\
           不要用 `Generic` —— 那个变体的分类永远是 `Unimplemented`，等于给\"消费掉但没做\"\n\
           留了一个合法外壳。"
    );
}
