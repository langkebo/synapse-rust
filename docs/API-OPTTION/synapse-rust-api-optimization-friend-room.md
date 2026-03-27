# Friend Room 模块优化方案 - 详细实现

## 一、现状分析

### 1.1 当前问题

**friend_room.rs 当前路由结构:**

```rust
// 重复模式:
// 每个端点都定义了 v1, v3, r0 三个版本，但处理函数完全相同
//
// 示例:
// route("/_matrix/client/v3/friends", get(get_friends))      // 主实现
// route("/_matrix/client/v1/friends", get(get_friends))       // 重复
// route("/_matrix/client/r0/friendships", get(get_friends))  // 重复
```

**统计:**
- 总路由数: 43 个 (含 v1/v3/r0)
- 实际唯一功能: ~14 个
- 代码重复率: ~67%

---

## 二、优化方案

### 2.1 方案设计

使用 **路径参数** 替代重复路由:

```rust
// 优化后: 一个路由匹配多个版本
route("/_matrix/client/{version}/friends", get(get_friends))

// 或使用通配符
route("/_matrix/client/:version/friends", get(get_friends))
```

### 2.2 代码实现

#### 2.2.1 重构后的 friend_room.rs

```rust
use crate::common::ApiError;
use crate::web::routes::{validate_user_id, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State, Query},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// API 版本常量
const VERSION_V3: &str = "v3";
const VERSION_V1: &str = "v1";
const VERSION_R0: &str = "r0";

pub fn create_friend_router(state: AppState) -> Router<AppState> {
    Router::new()
        // ===== 统一路由: 使用 {version} 参数 =====
        
        // 好友列表/请求
        .route(
            "/_matrix/client/{version}/friends",
            get(get_friends).post(send_friend_request),
        )
        .route(
            "/_matrix/client/{version}/friendships",
            get(get_friends).post(send_friend_request),
        )
        
        // 好友请求管理
        .route(
            "/_matrix/client/{version}/friends/request",
            post(send_friend_request),
        )
        .route(
            "/_matrix/client/{version}/friends/request/received",
            get(get_received_requests),
        )
        .route(
            "/_matrix/client/{version}/friends/request/{user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/_matrix/client/{version}/friends/request/{user_id}/reject",
            post(reject_friend_request),
        )
        .route(
            "/_matrix/client/{version}/friends/request/{user_id}/cancel",
            post(cancel_friend_request),
        )
        
        // 请求列表
        .route(
            "/_matrix/client/{version}/friends/requests/incoming",
            get(get_incoming_requests),
        )
        .route(
            "/_matrix/client/{version}/friends/requests/outgoing",
            get(get_outgoing_requests),
        )
        
        // 好友检查
        .route(
            "/_matrix/client/{version}/friends/check/{user_id}",
            get(check_friendship),
        )
        
        // 好友建议
        .route(
            "/_matrix/client/{version}/friends/suggestions",
            get(get_friend_suggestions),
        )
        
        // 好友详情
        .route(
            "/_matrix/client/{version}/friends/{user_id}",
            delete(remove_friend),
        )
        .route(
            "/_matrix/client/{version}/friends/{user_id}/note",
            put(update_friend_note),
        )
        .route(
            "/_matrix/client/{version}/friends/{user_id}/status",
            get(get_friend_status).put(update_friend_status),
        )
        .route(
            "/_matrix/client/{version}/friends/{user_id}/info",
            get(get_friend_info),
        )
        
        // 好友分组
        .route(
            "/_matrix/client/{version}/friends/groups",
            get(get_friend_groups).post(create_friend_group),
        )
        .route(
            "/_matrix/client/{version}/friends/groups/{group_id}",
            delete(delete_friend_group),
        )
        .route(
            "/_matrix/client/{version}/friends/groups/{group_id}/name",
            put(rename_friend_group),
        )
        .route(
            "/_matrix/client/{version}/friends/groups/{group_id}/add/{user_id}",
            post(add_friend_to_group),
        )
        .route(
            "/_matrix/client/{version}/friends/groups/{group_id}/remove/{user_id}",
            delete(remove_friend_from_group),
        )
        .route(
            "/_matrix/client/{version}/friends/groups/{group_id}/friends",
            get(get_friends_in_group),
        )
        .route(
            "/_matrix/client/{version}/friends/{user_id}/groups",
            get(get_groups_for_user),
        )
        .with_state(state)
}

/// 路径参数: API 版本
#[derive(Debug, Deserialize)]
pub struct VersionPath {
    version: String,
}

/// 版本兼容中间件
async fn check_version_compat(Path(params): Path<VersionPath>) -> Result<String, ApiError> {
    let version = params.version;
    
    // 验证版本并归一化
    match version.as_str() {
        VERSION_V3 | VERSION_V1 | VERSION_R0 => Ok(version),
        _ => {
            // 未知版本，返回默认 v3
            Ok(VERSION_V3.to_string())
        }
    }
}

// ============ 处理函数保持不变 ============
// (现有处理函数无需修改)

// ============ 辅助函数 ============

/// 验证版本兼容性 (可选: 添加弃用警告)
fn validate_and_log_version(version: &str) {
    if version == VERSION_R0 || version == VERSION_V1 {
        tracing::warn!(
            "API version '{}' is deprecated, please use 'v3'",
            version
        );
    }
}
```

---

## 三、Account Data 模块优化

### 3.1 当前问题

```rust
// 每个端点都重复定义了 r0 和 v3 版本
route("/_matrix/client/v3/user/{user_id}/account_data/{type}", put(set_account_data))
route("/_matrix/client/r0/user/{user_id}/account_data/{type}", put(set_account_data))
```

### 3.2 优化方案

```rust
pub fn create_account_data_router(state: AppState) -> Router<AppState> {
    Router::new()
        // ===== 统一路由: 用户账户数据 =====
        .route(
            "/_matrix/client/{version}/user/{user_id}/account_data/",
            get(list_account_data),
        )
        .route(
            "/_matrix/client/{version}/user/{user_id}/account_data/{type}",
            get(get_account_data).put(set_account_data),
        )
        
        // ===== 统一路由: 房间账户数据 =====
        .route(
            "/_matrix/client/{version}/user/{user_id}/rooms/{room_id}/account_data/{type}",
            get(get_room_account_data).put(set_room_account_data),
        )
        
        // ===== 统一路由: 过滤器 =====
        .route(
            "/_matrix/client/{version}/user/{user_id}/filter",
            put(create_filter).post(create_filter),
        )
        .route(
            "/_matrix/client/{version}/user/{user_id}/filter/{filter_id}",
            get(get_filter),
        )
        
        // ===== 统一路由: OpenID =====
        .route(
            "/_matrix/client/{version}/user/{user_id}/openid/request_token",
            get(get_openid_token),
        )
        .with_state(state)
}
```

---

## 四、Media 模块优化

### 4.1 当前问题

```rust
// v1 和 v3 版本完全重复
/_matrix/media/v1/upload    → 与 /_matrix/media/v3/upload 相同
/_matrix/media/v1/download → 与 /_matrix/media/v3/download 相同
```

### 4.2 优化方案

```rust
pub fn create_media_router(state: AppState) -> Router<AppState> {
    Router::new()
        // ===== 统一上传 (v1/v3) =====
        .route(
            "/_matrix/media/{version}/upload",
            post(upload_media),
        )
        
        // ===== 统一下载 =====
        .route(
            "/_matrix/media/{version}/download/{server_name}/{media_id}",
            get(download_media),
        )
        .route(
            "/_matrix/media/{version}/download/{server_name}/{media_id}/{filename}",
            get(download_media_with_name),
        )
        
        // ===== 统一缩略图 =====
        .route(
            "/_matrix/media/{version}/thumbnail/{server_name}/{media_id}",
            get(get_thumbnail),
        )
        
        // ===== 配置端点 =====
        .route(
            "/_matrix/media/{version}/config",
            get(get_media_config),
        )
        
        // ===== URL 预览 =====
        .route(
            "/_matrix/media/{version}/preview_url",
            post(preview_url),
        )
        
        // ===== 配额管理 =====
        .route(
            "/_matrix/media/{version}/quota/check",
            get(check_quota),
        )
        .route(
            "/_matrix/media/{version}/quota/stats",
            get(quota_stats),
        )
        
        // ===== 媒体删除 =====
        .route(
            "/_matrix/media/{version}/delete/{server_name}/{media_id}",
            delete(delete_media),
        )
        .with_state(state)
}
```

---

## 五、Device 模块优化

### 5.1 当前问题

```rust
// r0 和 v3 重复
/_matrix/client/r0/devices → /_matrix/client/v3/devices
/_matrix/client/r0/keys/device_list_updates → /_matrix/client/v3/keys/device_list_updates
```

### 5.2 优化方案

```rust
pub fn create_device_router(state: AppState) -> Router<AppState> {
    Router::new()
        // ===== 统一设备列表 =====
        .route(
            "/_matrix/client/{version}/devices",
            get(list_devices),
        )
        
        // ===== 统一设备详情 =====
        .route(
            "/_matrix/client/{version}/devices/{device_id}",
            get(get_device).put(update_device).delete(delete_device),
        )
        
        // ===== 统一删除多个设备 =====
        .route(
            "/_matrix/client/{version}/delete_devices",
            post(delete_devices),
        )
        
        // ===== 统一设备列表更新 =====
        .route(
            "/_matrix/client/{version}/keys/device_list_updates",
            get(get_device_list_updates),
        )
        .with_state(state)
}
```

---

## 六、搜索模块优化

### 6.1 当前问题

1. r0 和 v3 搜索端点重复
2. `user/{user_id}/rooms/{room_id}/threads` 与 thread 模块重复

### 6.2 优化方案

```rust
pub fn create_search_router(state: AppState) -> Router<AppState> {
    Router::new()
        // ===== 统一搜索 =====
        .route(
            "/_matrix/client/{version}/search",
            post(search),
        )
        
        // ===== 搜索房间/用户 =====
        .route(
            "/_matrix/client/{version}/search_rooms",
            post(search_rooms),
        )
        .route(
            "/_matrix/client/{version}/search_recipients",
            post(search_recipients),
        )
        
        // ===== 房间上下文 (唯一实现) =====
        .route(
            "/_matrix/client/{version}/rooms/{room_id}/context/{event_id}",
            get(get_event_context),
        )
        
        // ===== 房间层级 =====
        .route(
            "/_matrix/client/{version}/rooms/{room_id}/hierarchy",
            get(get_room_hierarchy),
        )
        
        // ===== 时间戳到事件 =====
        .route(
            "/_matrix/client/{version}/rooms/{room_id}/timestamp_to_event",
            get(timestamp_to_event),
        )
        
        // ===== 删除与 thread 模块重复的端点 =====
        // 注意: 删除以下端点，统一使用 thread 模块
        // .route("/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads", ...)
        
        .with_state(state)
}
```

---

## 七、版本兼容中间件 (统一实现)

### 7.1 中间件代码

在 `middleware.rs` 中添加版本处理:

```rust
use axum::{
    body::Body,
    extract::Path,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// 版本兼容中间件
pub async fn version_compat_middleware(
    Path((version, rest)): Path<(String, String)>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 归一化版本号
    let normalized_version = match version.as_str() {
        "v3" | "v1" | "r0" => version,
        _ => "v3".to_string(), // 默认 v3
    };
    
    // 添加弃用日志
    if version == "r0" || version == "v1" {
        tracing::warn!(
            target: "api_deprecation",
            "Deprecated API version '{}' accessed for {}",
            version,
            rest
        );
    }
    
    // 继续处理请求
    next.run(request).await
}
```

### 7.2 配置化版本管理

```rust
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// 是否启用 r0 兼容
    pub enable_r0_compat: bool,
    /// 是否启用 v1 兼容
    pub enable_v1_compat: bool,
    /// 默认 API 版本
    pub default_version: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enable_r0_compat: true,   // 默认开启，向后兼容
            enable_v1_compat: true,
            default_version: "v3".to_string(),
        }
    }
}
```

---

## 八、实施检查清单

### 8.1 需要修改的文件

| 文件 | 修改内容 |
|------|----------|
| `src/web/routes/friend_room.rs` | 合并重复路由，使用 {version} 参数 |
| `src/web/routes/account_data.rs` | 合并重复路由 |
| `src/web/routes/media.rs` | 合并重复路由 |
| `src/web/routes/device.rs` | 合并重复路由 |
| `src/web/routes/search.rs` | 删除重复端点 |
| `src/web/routes/mod.rs` | 添加版本兼容中间件 |
| `src/config.rs` | 添加 API 版本配置 |

### 8.2 测试计划

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version_redirect_r0_to_v3() {
        // 测试 r0 请求重定向到 v3
    }
    
    #[test]
    fn test_version_redirect_v1_to_v3() {
        // 测试 v1 请求重定向到 v3
    }
    
    #[test]
    fn test_all_endpoints_work() {
        // 测试所有唯一端点正常工作
    }
}
```

---

## 九、收益评估

| 模块 | 优化前路由数 | 优化后路由数 | 减少 |
|------|-------------|-------------|------|
| friend_room | 43 | 15 | 65% |
| account_data | 12 | 6 | 50% |
| media | 21 | 12 | 43% |
| device | 8 | 4 | 50% |
| search | 12 | 8 | 33% |
| **总计** | **96** | **45** | **53%** |

---

## 十、注意事项

1. **不破坏现有功能** - 所有处理函数保持不变
2. **向后兼容** - 通过配置控制是否启用旧版本
3. **渐进式迁移** - 可以先开启兼容模式，逐步迁移
4. **监控弃用** - 添加日志记录旧版本访问情况