//! 守卫：`scripts/api_test/` 不得再有「提交进仓库、却没人门控」的路由清单默认值。
//!
//! ## 为什么需要
//!
//! 历史缺陷（DOC-13 同型，2026-10-10 收口）：`scripts/api_test/ledger.json` 冻结在
//! 2026-08-12（1043 条，而同期真实导出是 948~1027 条不等），却有 4 个脚本把它当**默认输入**
//! （`run_api_tests.py` 经 `config.yaml#ledger_path`、`gen_client_yaml.py --skip-export`、
//! `refresh_openapi_specs.py --skip-export`、`generate_openapi.py` 的 profile 映射）。
//! 于是本地用这些脚本跑出来的一切**自成一体却是过期的** —— 与 DOC-13 的
//! `gen_route_table.py --ledger` 默认指向陈旧 `scripts/api_test/ledger.json` 是同一个根因。
//!
//! 修法也同款：**删掉默认值**（清单只能现导出，或由调用方显式给出），并用本测试把
//! 「别再加回来」变成可执行判据。
//!
//! ## 本测试能变红（铁律 8）
//!
//! * 恢复 `scripts/api_test/ledger.json`（例如手滑 `git checkout` 回来）⇒ 第一条断言失败；
//! * 把任一脚本的默认值改回裸 `"ledger.json"` ⇒ 第二条断言失败。

use std::fs;
use std::path::Path;

/// 路由清单只能来自 ① 现导出 或 ② 显式路径；不得有仓库文件默认值。
#[test]
fn api_test_scripts_have_no_committed_ledger_default() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("api_test");

    let stale = dir.join("ledger.json");
    assert!(
        !stale.exists(),
        "`{}` 又出现了：路由清单不得以仓库文件作为默认输入（现导出，或让调用方显式 --ledger）",
        stale.display()
    );

    // 只看「带引号的裸文件名」这一形态 —— 它正是各脚本里默认值的写法；
    // 散文里提到 `scripts/api_test/ledger.json` 的历史不在此列（那是文档，不是默认值）。
    for name in
        ["run_api_tests.py", "gen_client_yaml.py", "refresh_openapi_specs.py", "generate_openapi.py", "config.yaml"]
    {
        let path = dir.join(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()));
        assert!(
            !text.contains("\"ledger.json\"") && !text.contains("'ledger.json'"),
            "{name} 里又出现裸默认 `\"ledger.json\"`：应改为现导出，或指向 reports/ledger_default.json"
        );
    }
}
