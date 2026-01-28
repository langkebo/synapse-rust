# 模块结构文档

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、模块划分概述

### 1.1 模块组织结构

```
src/
├── lib.rs                    # 库入口，导出公共接口
├── main.rs                   # 二进制入口
├── common/                   # 通用模块
│   ├── mod.rs
│   ├── error.rs             # 错误类型定义
│   ├── config.rs            # 配置管理
│   └── crypto.rs           # 加密工具
├── storage/                  # 存储层模块
│   ├── mod.rs
│   ├── user.rs             # 用户存储
│   ├── device.rs           # 设备存储
│   ├── token.rs           # 令牌存储
│   ├── room.rs            # 房间存储
│   ├── event.rs           # 事件存储
│   ├── membership.rs       # 成员关系存储
│   ├── presence.rs        # 在线状态存储
│   ├── friend.rs         # 好友存储
│   ├── private.rs         # 私聊存储
│   └── voice.rs          # 语音消息存储
├── cache/                    # 缓存层模块
│   ├── mod.rs
│   └── mod.rs              # 缓存管理器
├── auth/                     # 认证模块
│   ├── mod.rs
│   └── mod.rs              # 认证服务
├── services/                 # 服务层模块
│   ├── mod.rs
│   ├── registration.rs     # 注册服务
│   ├── room_service.rs    # 房间服务
│   ├── sync_service.rs    # 同步服务
│   ├── media_service.rs   # 媒体服务
│   ├── friend_service.rs  # 好友服务
│   ├── private_chat.rs    # 私聊服务
│   └── voice_service.rs   # 语音服务
└── web/                      # Web 层模块
    ├── mod.rs
    ├── routes/             # 路由定义
    │   ├── mod.rs
    │   ├── client.rs      # 客户端 API 路由
    │   ├── admin.rs       # 管理 API 路由
    │   ├── media.rs       # 媒体 API 路由
    │   ├── friend.rs      # 好友 API 路由
    │   ├── private.rs     # 私聊 API 路由
    │   └── voice.rs      # 语音 API 路由
    ├── middleware/         # 中间件
    │   ├── mod.rs
    │   ├── auth.rs       # 认证中间件
    │   ├── logging.rs    # 日志中间件
    │   ├── cors.rs       # CORS 中间件
    │   └── rate_limit.rs # 速率限制中间件
    └── handlers/          # 请求处理器
        ├── mod.rs
        ├── client.rs     # 客户端 API 处理器
        ├── admin.rs      # 管理 API 处理器
        ├── media.rs      # 媒体 API 处理器
        ├── friend.rs     # 好友 API 处理器
        ├── private.rs    # 私聊 API 处理器
        └── voice.rs      # 语音 API 处理器
```

### 1.2 模块依赖关系

```
web/
  ├── routes/
  ├── middleware/
  └── handlers/
      ↓
services/
  ├── registration.rs
  ├── room_service.rs
  ├── sync_service.rs
  ├── media_service.rs
  ├── friend_service.rs
  ├── private_chat.rs
  └── voice_service.rs
      ↓
cache/
  └── mod.rs
      ↓
storage/
  ├── user.rs
  ├── device.rs
  ├── token.rs
  ├── room.rs
  ├── event.rs
  ├── membership.rs
  ├── presence.rs
  ├── friend.rs
  ├── private.rs
  └── voice.rs
      ↓
common/
  ├── error.rs
  ├── config.rs
  └── crypto.rs
```

---

## 二、Common 模块

### 2.1 模块职责

Common 模块提供项目通用的类型、工具和配置，被所有其他模块依赖。

### 2.2 子模块说明

#### 2.2.1 error.rs

**职责**：定义统一的错误类型和错误处理机制

**主要内容**：
- `ApiError`：公共错误类型
- 错误变体：`BadRequest`、`Unauthorized`、`Forbidden`、`NotFound`、`Internal` 等
- 错误转换：实现 `From` trait，支持从其他错误类型转换

**示例代码**：
```rust
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    
    #[error("Unauthorized")]
    Unauthorized,
    
    #[error("Forbidden")]
    Forbidden,
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}
```

#### 2.2.2 config.rs

**职责**：管理应用配置

**主要内容**：
- `Config`：配置结构体
- `DatabaseConfig`：数据库配置
- `CacheConfig`：缓存配置
- `ServerConfig`：服务器配置
- `Config::load()`：加载配置文件

**示例代码**：
```rust
#[derive(Debug, serde::Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub cache: CacheConfig,
    pub jwt: JwtConfig,
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
}
```

#### 2.2.3 crypto.rs

**职责**：提供加密和哈希工具

**主要内容**：
- `hash_password()`：密码哈希（Argon2）
- `verify_password()`：密码验证
- `generate_token()`：生成随机令牌
- `generate_room_id()`：生成房间 ID
- `generate_event_id()`：生成事件 ID

**示例代码**：
```rust
pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = Salt::random(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(password_hash.to_string())
}

pub fn generate_token(length: usize) -> String {
    let mut rng = OsRng;
    (0..length)
        .map(|_| rng.sample(Alphanumeric) as char)
        .collect()
}
```

---

## 三、Storage 模块

### 3.1 模块职责

Storage 模块负责所有数据库操作，提供类型安全的 SQL 查询接口。

### 3.2 子模块说明

#### 3.2.1 user.rs

**职责**：用户数据操作

**主要内容**：
- `User`：用户结构体
- `UserStorage<'a>`：用户存储 trait
- `create_user()`：创建用户
- `get_user()`：获取用户
- `get_user_by_username()`：根据用户名获取用户
- `update_user()`：更新用户
- `delete_user()`：删除用户

**示例代码**：
```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub admin: bool,
    pub deactivated: bool,
    pub is_guest: bool,
}

pub struct UserStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> UserStorage<'a> {
    pub async fn create_user(
        &self,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (user_id, username, password_hash, admin, creation_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            user_id,
            username,
            password_hash,
            is_admin,
            chrono::Utc::now().timestamp_millis()
        ).fetch_one(self.pool).await
    }
}
```

#### 3.2.2 device.rs

**职责**：设备数据操作

**主要内容**：
- `Device`：设备结构体
- `DeviceStorage<'a>`：设备存储 trait
- `create_device()`：创建设备
- `get_device()`：获取设备
- `get_user_devices()`：获取用户设备列表
- `update_device()`：更新设备
- `delete_device()`：删除设备
- `device_exists()`：检查设备是否存在

#### 3.2.3 token.rs

**职责**：令牌数据操作

**主要内容**：
- `AccessToken`：访问令牌结构体
- `RefreshToken`：刷新令牌结构体
- `TokenStorage<'a>`：令牌存储 trait
- `create_token()`：创建访问令牌
- `create_refresh_token()`：创建刷新令牌
- `get_token()`：获取令牌
- `invalidate_token()`：使令牌失效
- `delete_token()`：删除令牌

#### 3.2.4 room.rs

**职责**：房间数据操作

**主要内容**：
- `Room`：房间结构体
- `RoomStorage<'a>`：房间存储 trait
- `create_room()`：创建房间
- `get_room()`：获取房间
- `get_rooms()`：获取房间列表
- `update_room()`：更新房间
- `delete_room()`：删除房间

#### 3.2.5 event.rs

**职责**：事件数据操作

**主要内容**：
- `RoomEvent`：房间事件结构体
- `EventStorage<'a>`：事件存储 trait
- `create_event()`：创建事件
- `get_event()`：获取事件
- `get_room_events()`：获取房间事件列表
- `get_room_events_by_type()`：按类型获取房间事件
- `get_sender_events()`：获取发送者事件列表

#### 3.2.6 membership.rs

**职责**：成员关系数据操作

**主要内容**：
- `RoomMember`：房间成员结构体
- `MembershipStorage<'a>`：成员关系存储 trait
- `add_member()`：添加成员
- `remove_member()`：移除成员
- `get_members()`：获取成员列表
- `get_member()`：获取成员
- `update_membership()`：更新成员关系

#### 3.2.7 presence.rs

**职责**：在线状态数据操作

**主要内容**：
- `Presence`：在线状态结构体
- `PresenceStorage<'a>`：在线状态存储 trait
- `set_presence()`：设置在线状态
- `get_presence()`：获取在线状态
- `get_presences()`：获取在线状态列表

#### 3.2.8 friend.rs

**职责**：好友数据操作

**主要内容**：
- `Friend`：好友结构体
- `FriendRequest`：好友请求结构体
- `FriendCategory`：好友分类结构体
- `BlockedUser`：黑名单结构体
- `FriendStorage<'a>`：好友存储 trait
- `add_friend()`：添加好友
- `remove_friend()`：移除好友
- `get_friends()`：获取好友列表
- `send_friend_request()`：发送好友请求
- `respond_friend_request()`：响应好友请求

#### 3.2.9 private.rs

**职责**：私聊数据操作

**主要内容**：
- `PrivateSession`：私聊会话结构体
- `PrivateMessage`：私聊消息结构体
- `SessionKey`：会话密钥结构体
- `PrivateStorage<'a>`：私聊存储 trait
- `create_session()`：创建私聊会话
- `get_session()`：获取私聊会话
- `get_sessions()`：获取私聊会话列表
- `send_message()`：发送私聊消息
- `get_messages()`：获取私聊消息列表

#### 3.2.10 voice.rs

**职责**：语音消息数据操作

**主要内容**：
- `VoiceMessage`：语音消息结构体
- `VoiceStorage<'a>`：语音存储 trait
- `upload_voice()`：上传语音消息
- `get_voice()`：获取语音消息
- `get_user_voices()`：获取用户语音消息列表
- `delete_voice()`：删除语音消息

---

## 四、Cache 模块

### 4.1 模块职责

Cache 模块提供两级缓存机制，提升数据访问性能。

### 4.2 子模块说明

#### 4.2.1 mod.rs

**职责**：缓存管理器

**主要内容**：
- `CacheManager`：缓存管理器结构体
- `CacheConfig`：缓存配置
- `CacheManager::new()`：创建缓存管理器
- `get()`：获取缓存
- `set()`：设置缓存
- `delete()`：删除缓存
- `invalidate()`：失效缓存

**示例代码**：
```rust
use moka::future::Cache;
use redis::AsyncCommands;

pub struct CacheManager {
    local: Cache<String, String>,
    redis: Option<redis::aio::MultiplexedConnection>,
}

impl CacheManager {
    pub async fn get(&self, key: &str) -> Option<String> {
        if let Some(value) = self.local.get(key) {
            return Some(value);
        }
        if let Some(redis) = &self.redis {
            if let Ok(value) = redis.get::<_, String>(key).await {
                self.local.insert(key.to_string(), value.clone());
                return Some(value);
            }
        }
        None
    }
    
    pub async fn set(&self, key: &str, value: &str, ttl: Option<u64>) {
        self.local.insert(key.to_string(), value.to_string());
        if let Some(redis) = &self.redis {
            let _: () = redis.set_ex(key, value, ttl.unwrap_or(300)).await.unwrap();
        }
    }
}
```

---

## 五、Auth 模块

### 5.1 模块职责

Auth 模块提供认证和授权功能。

### 5.2 子模块说明

#### 5.2.1 mod.rs

**职责**：认证服务

**主要内容**：
- `AuthService`：认证服务结构体
- `AuthService::new()`：创建认证服务
- `register()`：用户注册
- `login()`：用户登录
- `logout()`：用户登出
- `validate_token()`：验证访问令牌
- `refresh_token()`：刷新访问令牌

**示例代码**：
```rust
pub struct AuthService {
    user_storage: UserStorage<'static>,
    device_storage: DeviceStorage<'static>,
    token_storage: TokenStorage<'static>,
    cache: Arc<CacheManager>,
    jwt_secret: Vec<u8>,
    access_token_expiry: i64,
    refresh_token_expiry: i64,
}

impl AuthService {
    pub async fn register(
        &self,
        username: &str,
        password: &str,
    ) -> Result<RegisterResponse, ApiError> {
        let password_hash = hash_password(password)?;
        let user_id = format!("@{}:{}", username, self.server_name);
        let user = self.user_storage.create_user(&user_id, username, Some(&password_hash), false).await?;
        let device_id = generate_token(16);
        self.device_storage.create_device(&device_id, &user_id, None, None).await?;
        let access_token = self.generate_access_token(&user_id, &device_id).await?;
        Ok(RegisterResponse {
            user_id: user.user_id,
            access_token,
            device_id,
        })
    }
}
```

---

## 六、Services 模块

### 6.1 模块职责

Services 模块封装业务逻辑，协调存储层和缓存层的操作。

### 6.2 子模块说明

#### 6.2.1 registration.rs

**职责**：注册服务

**主要内容**：
- `RegistrationService`：注册服务结构体
- `register()`：用户注册
- `validate_username()`：验证用户名
- `validate_password()`：验证密码

#### 6.2.2 room_service.rs

**职责**：房间服务

**主要内容**：
- `RoomService`：房间服务结构体
- `create_room()`：创建房间
- `join_room()`：加入房间
- `leave_room()`：离开房间
- `invite_user()`：邀请用户
- `send_message()`：发送消息

#### 6.2.3 sync_service.rs

**职责**：同步服务

**主要内容**：
- `SyncService`：同步服务结构体
- `sync()`：同步事件
- `get_events()`：获取事件
- `filter_events()`：过滤事件

#### 6.2.4 media_service.rs

**职责**：媒体服务

**主要内容**：
- `MediaService`：媒体服务结构体
- `upload_media()`：上传媒体文件
- `download_media()`：下载媒体文件
- `delete_media()`：删除媒体文件

#### 6.2.5 friend_service.rs

**职责**：好友服务

**主要内容**：
- `FriendService`：好友服务结构体
- `get_friends()`：获取好友列表
- `send_friend_request()`：发送好友请求
- `respond_friend_request()`：响应好友请求
- `add_friend()`：添加好友
- `remove_friend()`：移除好友

#### 6.2.6 private_chat.rs

**职责**：私聊服务

**主要内容**：
- `PrivateChatService`：私聊服务结构体
- `create_session()`：创建私聊会话
- `send_message()`：发送私聊消息
- `get_messages()`：获取私聊消息
- `mark_as_read()`：标记消息已读

#### 6.2.7 voice_service.rs

**职责**：语音服务

**主要内容**：
- `VoiceService`：语音服务结构体
- `upload_voice()`：上传语音消息
- `get_voice()`：获取语音消息
- `get_user_voices()`：获取用户语音消息列表
- `delete_voice()`：删除语音消息

---

## 七、Web 模块

### 7.1 模块职责

Web 模块处理 HTTP 请求和响应，使用 Axum 框架实现。

### 7.2 子模块说明

#### 7.2.1 routes/

**职责**：定义 API 路由

**主要内容**：
- `client.rs`：客户端 API 路由
- `admin.rs`：管理 API 路由
- `media.rs`：媒体 API 路由
- `friend.rs`：好友 API 路由
- `private.rs`：私聊 API 路由
- `voice.rs`：语音 API 路由

**示例代码**：
```rust
use axum::Router;

pub fn create_client_router() -> Router {
    Router::new()
        .route("/_matrix/client/versions", axum::routing::get(get_versions))
        .route("/_matrix/client/r0/register", axum::routing::post(register))
        .route("/_matrix/client/r0/login", axum::routing::post(login))
        .route("/_matrix/client/r0/logout", axum::routing::post(logout))
        .route("/_matrix/client/r0/sync", axum::routing::get(sync))
        .route("/_matrix/client/r0/createRoom", axum::routing::post(create_room))
}
```

#### 7.2.2 middleware/

**职责**：实现中间件

**主要内容**：
- `auth.rs`：认证中间件
- `logging.rs`：日志中间件
- `cors.rs`：CORS 中间件
- `rate_limit.rs`：速率限制中间件

**示例代码**：
```rust
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};

pub async fn auth_middleware(
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_token(&req)?;
    let state = req.extract::<State<AppState>>().await?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;
    req.extensions_mut().insert(user_id);
    Ok(next.run(req).await)
}
```

#### 7.2.3 handlers/

**职责**：实现请求处理器

**主要内容**：
- `client.rs`：客户端 API 处理器
- `admin.rs`：管理 API 处理器
- `media.rs`：媒体 API 处理器
- `friend.rs`：好友 API 处理器
- `private.rs`：私聊 API 处理器
- `voice.rs`：语音 API 处理器

**示例代码**：
```rust
use axum::{extract::State, Json, response::Json as ResponseJson};

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<ResponseJson<RegisterResponse>, ApiError> {
    let response = state.services.registration_service.register(req).await?;
    Ok(ResponseJson(response))
}
```

---

## 八、模块接口定义

### 8.1 Storage Trait

```rust
#[async_trait]
pub trait Storage<'a> {
    type Error;
    
    async fn create(&self, entity: Self::Entity) -> Result<Self::Entity, Self::Error>;
    async fn get(&self, id: &str) -> Result<Option<Self::Entity>, Self::Error>;
    async fn update(&self, entity: Self::Entity) -> Result<Self::Entity, Self::Error>;
    async fn delete(&self, id: &str) -> Result<(), Self::Error>;
}
```

### 8.2 Service Trait

```rust
#[async_trait]
pub trait Service {
    type Request;
    type Response;
    type Error;
    
    async fn handle(&self, request: Self::Request) -> Result<Self::Response, Self::Error>;
}
```

### 8.3 Cache Trait

```rust
#[async_trait]
pub trait Cache {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>);
    async fn delete(&self, key: &str);
    async fn invalidate(&self, pattern: &str);
}
```

---

## 九、参考资料

- [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)
- [Matrix 规范](https://spec.matrix.org/)
- [Axum 框架文档](https://docs.rs/axum/latest/axum/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)

---

## 十、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义模块结构文档 |
