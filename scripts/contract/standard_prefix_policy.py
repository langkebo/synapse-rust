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

# `/_matrix/client/unstable/<org>.<name>[.vN]/...` —— MSC 端点借用的临时前缀。
# 其**第 4 段是 `版本/归属位`，不是路径的一节**，故与 vendor 孪生配对时必须先剥掉它
# （否则 `unstable/uk.half-shot.msc2666/user/mutual_rooms` 与
# `/_matrix/vendor/v1/user/mutual_rooms` 长得完全不一样，同 handler 的死别名会被漏掉）。
UNSTABLE_PREFIX = "/_matrix/client/unstable"

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
    # ── 2026-10-08 M0 补登（G-01 扩展到 `/unstable/` 后由判据报出的 2 条）──────────
    # 两条都**不是**死别名：一条有活的 SDK 消费者，另一条是刻意的兼容位。
    ("GET", "/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms"): (
        2666,
        "MSC2666 mutual rooms 的 **unstable 特性探测位** —— SDK 的 server-capabilities "
        "用它判断服务端是否支持（matrix-js-sdk/src/server-capabilities/index.ts:424），"
        "稳定入口是 `/_matrix/vendor/v1/user/mutual_rooms`，两者用途不同 ⇒ 保留",
    ),
    ("GET", "/_matrix/client/unstable/org.matrix.msc4156/threads/subscribed"): (
        4156,
        "线程订阅（用户私有态）的 **unstable 兼容位**，仅为已发布客户端保留 "
        "（handlers/thread.rs:191-196 自述：它不是 MSC4156 表面）；主路径为 "
        "`/_matrix/client/v1/threads/subscribed`",
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

# 混合模块（既有标准端点也有私有端点）—— 这些源文件里的 client 前缀路由必须逐条出现在
# `mixed_module_client_routes.txt` 的**冻结清单**里（双向 ratchet，见该文件头注）。
#
# 与 `WHOLESALE_PRIVATE_FILES` 的分工：那些是"整模块私有"，可做全覆盖断言；混合模块的
# 每条都要先判"是不是规范端点"，而那需要一个**独立于后端注册面**的来源，故外置为清单
# 文件（G-02/G-03，2026-10-08）。上面的 `MIXED_MODULE_ROUTES` 只记"*已识别的*私有端点"
# （人工登记），本常量 + 清单才是**全覆盖**断言。
#
# ⚠️ 覆盖范围只有这 3 个文件 —— 它**不是**全仓混检：其余含 client 前缀路由的文件
# （`assembly.rs` 等挂载汇总、`key_backup.rs`、`e2ee/keys.rs` …）仍未做该断言。
# 扩大范围 = 往本元组 + 清单文件里追加，判据本身无需改。
MIXED_MODULE_FILES = (
    "room.rs",
    "moderation.rs",
    "handlers/thread.rs",
)

# 冻结清单文件名（与 `standard_prefix_ledger.txt` 同目录）。
MIXED_MODULE_CLIENT_ROUTES_FILE = "mixed_module_client_routes.txt"

# 冻结清单里标 `private` 的**条数上限**（只减不增；与 `LEDGER_CEILING` 同型）。
#
# 口径：`mixed_module_client_routes.txt` 中第 3 列为 `private` 的行数。
# 2026-10-08 首测 = **55**（`room.rs` 54 + `handlers/thread.rs` 1）。
# 2026-10-08 M1：删 `POST /_matrix/client/v1/rooms/create_private`（与 v3 同 handler 的
# 版本孪生，SDK 只打 v3）⇒ 55 → **54**、清单 105 → 104 条。
# 2026-10-08 M2：把 8 条**移出 client v1/v3 前缀**（MSC4354 sticky ×3 与 MSC3946 pinned ×3
# 归位到 `/_matrix/client/unstable/org.matrix.msc{4354,3946}`；`thread/{event_id}` 与
# `unfreeze` 归位 `/_matrix/vendor/v1`）⇒ 54 → **46**、清单 104 → 96 条。
#
# ⚠️ 只有真的把它们删掉或迁到 `/_matrix/vendor/v1` 才允许下调；判据 D 已经从
# "集合"维度把住增删，这个数字是**数值**维度的第二道护栏（集合判据被误改时仍能兜住）。
MIXED_MODULE_PRIVATE_COUNT = 46


def normalize(path: str) -> str:
    """剥掉命名空间前缀，便于把 client 路由与 vendor 孪生配对。

    `my_rooms` 的 vendor 孪生在 `assembly.rs` 的 `create_vendor_router()` 里、
    client 别名在 `sync.rs` 里 —— 所以配对必须**全局**做，不能按源文件做。
    """
    for prefix in (VENDOR_PREFIX, *CLIENT_PREFIX_BASES):
        if path.startswith(prefix):
            return path[len(prefix) :]
    return path


def relative_under_client(path: str) -> str | None:
    """把 **client 侧**路径归一成"相对路径"，用于与 vendor 孪生配对（G-01 扩展）。

    与 `normalize()` 的区别：本函数额外处理 `unstable` 前缀，且对非 client 侧返回 `None`
    （`normalize()` 对任何输入都返回一个字符串，配对时会把 vendor 路径也算进来）。

    - `/_matrix/client/v3/rooms/x`          → `/rooms/x`
    - `/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms`
                                            → `/user/mutual_rooms`（第 4 段是版本/归属位，剥掉）
    - `/_matrix/vendor/v1/x`                → `None`

    ⚠️ 这是 2026-10-08 补上的盲区：原判据只比 `CLIENT_PREFIX_BASES`（v1/v3/r0），
    于是 `unstable/uk.half-shot.msc2666/user/mutual_rooms` 与 vendor 的
    `/user/mutual_rooms` **挂同一 handler** 却长期未被发现。
    """
    for base in CLIENT_PREFIX_BASES:
        if path.startswith(base + "/"):
            return path[len(base) :]
    if path.startswith(UNSTABLE_PREFIX + "/"):
        rest = path[len(UNSTABLE_PREFIX) + 1 :]
        _, sep, tail = rest.partition("/")
        return "/" + tail if sep else None
    return None


def classify(method: str, path: str, vendor_twins: set):
    """返回 `(policy, action, reason)`。`vendor_twins` 是全局 `(method, 相对路径)` 集合。"""
    if (method, path) in MSC_KEEP:
        msc, why = MSC_KEEP[(method, path)]
        return f"msc{msc}", "keep", why
    if (method, normalize(path)) in vendor_twins:
        return "vendor", "delete-alias", "已有 vendor 孪生，client 侧为死别名"
    return "vendor", "move-to-vendor", "私有端点，需迁 vendor"
