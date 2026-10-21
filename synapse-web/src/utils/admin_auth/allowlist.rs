//! RBAC 允许面：**声明式数据表**。
//!
//! 这块原来是 `admin_auth.rs` 里一条 130 行的 `||` 链。链有两个问题：
//!
//! 1. 每加一个端点就要往函数体里塞条件，而文件压在 600 行棘轮上——加规则和拆文件
//!    变成互相打架的两件事；
//! 2. `||` 链**读不出结构**：想知道"`media_admin` 能碰什么"，只能从头顺着读；
//!    想知道"`/media` 都谁能碰"，只能靠 `grep` 前缀。
//!
//! 现在每一行就是 `(角色, 方法类别, 路径形态)` 的三元组，顺序无关，可 grep、可 diff。
//!
//! 判定语义与拆分前**逐条等价**，由 `admin_rbac_tests` 的三张 golden 钉住：
//! 写面登记表、读面登记表、以及 `role × 每条 admin 路由` 的判定矩阵。
//! 也就是说：这张表怎么改都可以，**只要矩阵 diff 是你想要的那几行**。

use axum::http::Method;

/// 方法类别。
///
/// 只列实际用到的三种形态。刻意**不**做成位图：多一种形态就该多一个名字，
/// 免得出现 `Read | Delete | Patch` 这种"看起来能读其实还带写"的位组合。
#[derive(Clone, Copy)]
pub(super) enum MethodClass {
    /// 任意方法。
    Any,
    /// 只读（GET / HEAD）。
    Read,
    /// 只读，或单个设备的 DELETE（批量删除除外，由 [`PathMatch::UserDevices`] 排除）。
    ReadOrDelete,
}

impl MethodClass {
    fn allows(self, method: &Method) -> bool {
        match self {
            Self::Any => true,
            Self::Read => matches!(*method, Method::GET | Method::HEAD),
            Self::ReadOrDelete => matches!(*method, Method::GET | Method::HEAD | Method::DELETE),
        }
    }
}

/// 路径形态。
pub(super) enum PathMatch {
    /// 前缀匹配。注意**不认边界**：`/v1/users` 会命中 `/v1/users_extra`。
    /// 这是既有语义，改动它等于改变整张允许面。
    Prefix(&'static str),
    /// 精确匹配（用来表达"集合端点"，以及那些不该被子路径顺带放行的单点）。
    Exact(&'static str),
    /// 包含匹配。只用于 `/registration_tokens`：它出现在 v1 路径中段，
    /// 且只在只读时放行。
    Contains(&'static str),
    /// 前缀 + 后缀。
    PrefixAndSuffix(&'static str, &'static str),
    /// 单个设备操作：`/users/{id}/devices...`，但**批量删除** `…/delete` 除外。
    /// 批量删除早就收到 super_admin（P1-1），这里保持一致。
    UserDevices,
}

impl PathMatch {
    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Prefix(prefix) => path.starts_with(prefix),
            Self::Exact(exact) => path == *exact,
            Self::Contains(needle) => path.contains(needle),
            Self::PrefixAndSuffix(prefix, suffix) => path.starts_with(prefix) && path.ends_with(suffix),
            Self::UserDevices => path.contains("/users/") && path.contains("/devices") && !path.ends_with("/delete"),
        }
    }
}

/// 一条授权：方法类别 + 路径形态。
pub(super) struct Grant {
    methods: MethodClass,
    path: PathMatch,
}

impl Grant {
    fn allows(&self, method: &Method, path: &str) -> bool {
        self.methods.allows(method) && self.path.matches(path)
    }
}

const fn g(methods: MethodClass, path: PathMatch) -> Grant {
    Grant { methods, path }
}

use MethodClass::{Any, Read, ReadOrDelete};
use PathMatch::{Contains, Exact, Prefix, PrefixAndSuffix, UserDevices};

/// `admin`：内容治理 + 日常运维。**不含**凭据面、可加载代码、全局集成配置
/// （那批由 `privileged.rs` 的黑名单挖掉）。逐条理由见
/// `docs/audit/ADMIN_RBAC_SURFACE_2026-10-21.md`。
const ADMIN_GRANTS: &[Grant] = &[
    // 用户（读 + 运维；目标为管理员时的守卫在 handler 层，见文档 §4.2）
    g(Any, Prefix("/_synapse/admin/v1/users")),
    g(Any, Prefix("/_synapse/admin/v2/users")),
    g(Any, Prefix("/_synapse/admin/v1/user_sessions")),
    g(Any, Prefix("/_synapse/admin/v1/user_stats")),
    g(Any, Prefix("/_synapse/admin/v1/account/")),
    g(Any, Prefix("/_synapse/admin/v1/notifications")),
    // 媒体治理（含隔离/解除，均可逆）
    g(Any, Prefix("/_synapse/admin/v1/media")),
    // 房间治理
    g(Any, Prefix("/_synapse/admin/v1/rooms")),
    g(Any, Exact("/_synapse/admin/v1/shutdown_room")),
    // 房间统计：前缀不带结尾斜杠，否则集合端点会被漏掉（见文档 §9.2）
    g(Any, Prefix("/_synapse/admin/v1/room_stats")),
    // 注册令牌：只读（令牌增删是部署级动作，只给 super_admin）
    g(Read, Contains("/registration_tokens")),
    // 统计
    g(Any, Prefix("/_synapse/admin/v1/statistics")),
    g(Any, Prefix("/_synapse/admin/v1/stats")),
    // 清理（破坏性强的 `cleanup/all`、`cleanup/rooms` 已在黑名单）
    g(Any, Prefix("/_synapse/admin/v1/cleanup")),
    // 后台任务
    g(Any, Prefix("/_synapse/admin/v1/background_updates")),
    // 举报处置
    g(Any, Prefix("/_synapse/admin/v1/event_reports")),
    // 空间
    g(Any, Prefix("/_synapse/admin/v1/spaces")),
    // 功能开关：读（写会关闭安全特性，见黑名单）
    g(Any, Prefix("/_synapse/admin/v1/experimental_features")),
    g(Any, Prefix("/_synapse/admin/v1/feature_flags")),
    g(Any, Prefix("/_synapse/admin/v1/feature-flags")),
    // 应用服务：读（`as_token` 增删改在黑名单；响应已裁剪掉 token 字段）
    g(Any, Prefix("/_synapse/admin/v1/appservices")),
    // 审计日志：只读（手写审计事件会污染审计链，见黑名单）
    g(Read, Prefix("/_synapse/admin/v1/audit")),
    // 单设备操作：读 + 删单个设备；批量删除交给黑名单
    g(ReadOrDelete, UserDevices),
    // 联邦治理
    g(Any, Prefix("/_synapse/admin/v1/federation")),
    // CAS：读 + 用户属性（服务增删是 SSO 配置，见黑名单）
    g(Any, Prefix("/_synapse/admin/v1/cas")),
    // worker / room_summary 内部面
    g(Any, Prefix("/_synapse/worker/v1/")),
    g(Any, Prefix("/_synapse/room_summary/v1/")),
    // vendor 密钥轮换（ISSUE-13：`/_matrix/client/v1/keys/rotation` 孪生别名已删，
    // 别再把死路径写回来）
    g(Any, Prefix("/_matrix/vendor/v1/keys/rotation")),
    // 版本 / 健康 / 状态
    g(Any, Prefix("/_synapse/admin/v1/server_version")),
    g(Any, Prefix("/_synapse/admin/v1/health")),
    g(Any, Prefix("/_synapse/admin/v1/status")),
    // 模块：读 + `check_*` 探测（可加载代码的那批在黑名单）
    g(Any, Prefix("/_synapse/admin/v1/modules")),
    // 账号有效期：单人 renew 给 admin，全站批量在黑名单
    g(Any, Prefix("/_synapse/admin/v1/account_validity")),
    // SAML：读 + mapping 读（配置与映射写收紧，见黑名单/§4.1）
    g(Any, Prefix("/_synapse/admin/v1/saml")),
    // 外部服务 / webhook：读 + health 探测（注册/改/删收紧，见 §4.1）
    g(Any, Prefix("/_synapse/admin/v1/external_services")),
    g(Any, Prefix("/_synapse/admin/v1/external_webhook")),
    // 遥测 / whois
    g(Any, Prefix("/_synapse/admin/v1/telemetry")),
    g(Any, Prefix("/_synapse/admin/v1/whois")),
    // 2026-10-21（第一批）：这四类此前无任何前缀覆盖 ⇒ 普通 admin 完全不可用（过严）
    g(Any, Prefix("/_synapse/admin/v1/invite/")),
    g(Any, Exact("/_synapse/admin/v1/captcha/cleanup")),
    g(Any, Prefix("/_synapse/admin/v1/server_notices")),
    g(Any, Prefix("/_synapse/admin/v1/reports/")),
    g(Any, Exact("/_synapse/admin/v1/reports")),
    // 2026-10-21（第二批）：路径别名与规范路径必须同判定
    g(Any, Prefix("/_synapse/admin/v1/deactivate/")),
    g(Any, PrefixAndSuffix("/_synapse/admin/v1/user/", "/media/quarantine")),
    // 2026-10-21（读面）：`quarantine_media/{id}/changes` 是 `media/quarantine_changes`
    // 的旧别名；下面三条只回调用者自己的身份/限流/策略开关
    g(Any, Prefix("/_synapse/admin/v1/quarantine_media/")),
    g(Any, Exact("/_synapse/admin/v1/whoami")),
    g(Any, Exact("/_synapse/admin/v1/rate-limit-status")),
    g(Any, Exact("/_synapse/admin/v1/policy/status")),
];

/// `auditor`：只看，且只看审计相关。
const AUDITOR_GRANTS: &[Grant] = &[
    g(Read, Prefix("/_synapse/admin/v1/audit")),
    g(Read, Prefix("/_synapse/admin/v1/event_reports")),
    g(Read, Prefix("/_synapse/admin/v1/security")),
];

/// `security_admin`：安全与审计的读写（联邦、房间等仍需 `admin`）。
const SECURITY_ADMIN_GRANTS: &[Grant] = &[
    g(Any, Prefix("/_synapse/admin/v1/security")),
    g(Any, Prefix("/_synapse/admin/v1/audit")),
    g(Any, Prefix("/_synapse/admin/v1/event_reports")),
    g(Any, Prefix("/_synapse/admin/v1/server")),
];

/// `user_admin`：只管用户。
const USER_ADMIN_GRANTS: &[Grant] = &[
    g(Any, Prefix("/_synapse/admin/v1/users")),
    g(Any, Prefix("/_synapse/admin/v2/users")),
    g(Any, Prefix("/_synapse/admin/v1/notifications")),
];

/// `media_admin`：只管媒体。
const MEDIA_ADMIN_GRANTS: &[Grant] = &[g(Any, Prefix("/_synapse/admin/v1/media"))];

/// 角色 → 允许面。`None` = 这个角色没有任何权限（含 `user_type` 未设置的 `none`）。
pub(super) fn role_grants(role: &str) -> Option<&'static [Grant]> {
    match role {
        "admin" => Some(ADMIN_GRANTS),
        "auditor" => Some(AUDITOR_GRANTS),
        "security_admin" => Some(SECURITY_ADMIN_GRANTS),
        "user_admin" => Some(USER_ADMIN_GRANTS),
        "media_admin" => Some(MEDIA_ADMIN_GRANTS),
        _ => None,
    }
}

/// 该角色是否被授予 `method + path`。**不含**黑名单判定与 `super_admin` 短路，
/// 那两步在调用方（`is_role_allowed`）里，保证所有角色都先过一遍黑名单。
pub(super) fn role_allows(role: &str, method: &Method, path: &str) -> bool {
    role_grants(role).is_some_and(|grants| grants.iter().any(|grant| grant.allows(method, path)))
}
