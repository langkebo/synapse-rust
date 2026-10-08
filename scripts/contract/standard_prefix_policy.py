#!/usr/bin/env python3
"""标准前缀策略：ISSUE-13「私有端点不得污染 `/_matrix/client/{v1,v3}`」的判定数据。

**单一真相源。** 两个消费者共用本模块，不得各自复制一份清单（AGENTS.md 铁律 2）：

* `gen_standard_prefix_ledger.py` —— 按这里的规则生成台账
  `standard_prefix_ledger.txt`；
* `test_extract_registered.py::check_standard_prefix_bucket` —— 按这里的
  `WHOLESALE_PRIVATE_FILES` 做全覆盖断言，并按 `LEDGER_CEILING` 卡"只减不增"。

为什么判定数据必须按**路径形状**而不是模块名
------------------------------------------
`room.rs` 有 97 条 client 前缀路由，其中 93 条是标准端点；
`handlers/thread.rs` 有 21 条，其中 18 条是 MSC3856。
用"模块是不是私有模块"来判定，这两处都会判错 —— 这正是旧门禁
（`matrix-js-sdk/scripts/quality/check-vendor-prefix-migration.mjs` 里
5 个模块名的硬编码清单）失效的原因，也是 ISSUE-13 原计划要求的是
"spec 路径白名单"而不是模块清单的原因。

分类依据与逐条证据见 `docs/前缀命名空间治理方案-2026-10-08.md` §1。
"""

# client 前缀（带尾斜杠，便于 path.startswith 判定）。私有扩展不该出现在这里。
CLIENT_PREFIXES = ("/_matrix/client/v1/", "/_matrix/client/v3/")

# 不含尾斜杠的形态，用于 strip 出相对路径与 vendor 孪生比对。
CLIENT_PREFIX_BASES = ("/_matrix/client/v1", "/_matrix/client/v3", "/_matrix/client/r0")

VENDOR_PREFIX = "/_matrix/vendor/v1"

# 整模块私有：这些源文件只服务私有扩展，没有标准端点。因此它们**全部**
# client 前缀路由都必须登记台账 —— 于是"在这些模块里新增一个私有端点
# 而不登记"会立刻变红，这是本门禁堵住的现实回潮路径。
WHOLESALE_PRIVATE_FILES = (
    "dm.rs",
    "friend_room.rs",
    "burn_after_read.rs",
    "key_rotation.rs",
    "push_notification.rs",
    "room_summary.rs",
    "voice.rs",
    "widget.rs",
    "space/children_hierarchy.rs",
    "space/lifecycle_query.rs",
    "space/membership_state.rs",
    "space/summary.rs",
)

# 整模块私有桶的 client 前缀路由条数上限（只减不增）。
#
# 口径：`WHOLESALE_PRIVATE_FILES` 里的文件在 `/_matrix/client/{v1,v3}` 下的路由条数。
# 与 policy 标注**无关** —— 因此改标签绕不过去，只有真的删掉或迁走路由才能降下来。
#
# 2026-10-08 实测 = 169，其中 5 条是 MSC 端点（合法保留），故理论下限是 5。
# 初始值刻意取**实测值本身**而不是留余量：留余量等于给"再污染 20 条"发通行证，
# 而本门禁的全部意义就是不让你再加。
#
# 2026-10-08 Phase 2 batch 1：删除 77 条 client 死别名（friend_room 36 / voice 18 /
# burn_after_read 14 / key_rotation 9，全部是已有 vendor 孪生的别名），169 → 92。
# 台账同步 189 → 112 条（92 + 20 混合模块）。
#
# 2026-10-08 Phase 2 batch 2：把 78 条私有端点从 client 前缀**迁到** vendor
# （space 44 / widget 18 / room_summary 16；其中 space 的 44 条是 22 条路径被
# nest 到 v1+v3 两次，迁移后收敛为 22 条），92 → 14（= 5 条 MSC keep + 9 条留给
# 第三批的 dm 5 + push_notification 4）。台账同步 112 → 34 条（14 + 20 混合模块）。
#
# 2026-10-08 Phase 3（第三批）：把最后 29 条私有端点迁到 vendor
# （handlers/thread 14 / dm 5 / push_notification 4 / moderation 3 / room 3），
# 14 → **5**（只剩 MSC2946×4 + MSC3266×1）。台账 34 → 5 条。
# ⚠️ 注册条目净 −1（1,036 → 1,035）：moderation 的 `PUT .../report/{event_id}/score`
# 原先在 v1/v3 各挂一份，迁到 vendor 后合为一条 —— vendor 只有一个版本。
#
# ⚠️ 这个数字只在路由**真正迁走/删除**时才下调（并同步收紧台账）。
# 路由真的变少却不下调，门禁会一直报"未收紧"——那是提醒，不是误报。
LEDGER_CEILING = 5

# 有 MSC 归属、合法留在 client 前缀的端点。进台账只为记账（防止被后人误判成污染），
# action=keep，不产生"只减不增"的压力。
MSC_KEEP = {
    ("GET", "/_matrix/client/v1/spaces/{space_id}/hierarchy"): (
        2946,
        "MSC2946 spaces hierarchy（已稳定）",
    ),
    ("GET", "/_matrix/client/v1/spaces/{space_id}/hierarchy/v1"): (
        2946,
        "MSC2946 早期形状",
    ),
    ("GET", "/_matrix/client/v3/spaces/{space_id}/hierarchy"): (
        2946,
        "MSC2946 spaces hierarchy（已稳定）",
    ),
    ("GET", "/_matrix/client/v3/spaces/{space_id}/hierarchy/v1"): (
        2946,
        "MSC2946 早期形状",
    ),
    ("GET", "/_matrix/client/v1/rooms/{room_id}/summary"): (
        3266,
        "MSC3266 room summary",
    ),
}

# 混合模块（既有标准端点也有私有端点）里已人工识别出的私有端点。
# 这些模块**不**做全覆盖断言：那需要一份机器可读的 spec 路径清单，
# 属独立议题（见方案 §3 Phase 1 的讨论；落地项 = Batch 4 的 G-02/G-03）。
#
# 2026-10-08 Phase 3：原先登记的 20 条（handlers/thread.rs 14 + moderation.rs 3 +
# room.rs 3）**已全部迁到 `/_matrix/vendor/v1`**，故本登记位清空。
#
# 刻意保留这个（当前为空的）登记位而不是删掉它 —— 它是"混合模块里发现的私有
# 端点"的**唯一登记入口**（`gen_standard_prefix_ledger.py:109` 按它渲染台账理由），
# 删掉会让下一次发现无处可记。同时请正视它的**能力边界**：只有登记进来的才会
# 被看见，它不是全覆盖断言；那个方向由 Batch 4 的 G-02/G-03 补。
MIXED_MODULE_ROUTES: dict[str, dict[tuple[str, str], str]] = {}


def normalize(path: str) -> str:
    """剥掉命名空间前缀，便于把 client 路由与 vendor 孪生配对。

    `my_rooms` 的 vendor 孪生在 `assembly.rs` 的 `create_vendor_router()` 里、
    client 别名在 `sync.rs` 里 —— 所以配对必须**全局**做，不能按源文件做。
    """
    for prefix in (VENDOR_PREFIX, *CLIENT_PREFIX_BASES):
        if path.startswith(prefix):
            return path[len(prefix) :]
    return path


def classify(method: str, path: str, vendor_twins: set):
    """返回 `(policy, action, reason)`。`vendor_twins` 是全局 `(method, 相对路径)` 集合。"""
    if (method, path) in MSC_KEEP:
        msc, why = MSC_KEEP[(method, path)]
        return f"msc{msc}", "keep", why
    if (method, normalize(path)) in vendor_twins:
        return "vendor", "delete-alias", "已有 vendor 孪生，client 侧为死别名"
    return "vendor", "move-to-vendor", "私有端点，需迁 vendor"
