# 上游 Synapse v12 房间版本与动画缩略图实现方案

**生成时间**: 2026-09-25  
**状态**: 研究完成，等待用户决策  

---

## 一、研究结论摘要

### 1.1 v12 房间版本

**上游现状** (Synapse v1.162.0rc1):
- ✅ **默认版本已提升至 v12** (`CHANGES.md`: "Raise default room version to '12'")
- ✅ **核心 MSC**: MSC4239 (v12 定义)、MSC4311 (邀请/敲击状态)、MSC3912 (基于关系的撤回)
- ⚠️ **安全漏洞**: CVE-2025-49090 (具体细节未在发布说明中公开)
- 🔑 **关键变更**:
  - ED25519-only 签名验证 (更严格的算法白名单)
  - PDU 验证增强 (`depth`、`prev_events`、`auth_events` 必须正确填充)
  - MSC4311: `m.room.create` 现在必须出现在 v12 房间的 stripped invite/knock state 中

**我们项目现状**:
```rust
// synapse-common/src/room_versions.rs
pub const DEFAULT_ROOM_VERSION: &str = "11";  // ❌ 仍是 v11

RoomVersionCapability::stable_parse_only("12"),  // ❌ 不可创建
RoomVersionCapability::stable_parse_only("13"),  // ❌ 不可创建
```

**差距**:
1. ❌ 无法创建 v12 房间 (`stable_parse_only`)
2. ❌ `create_event` 不填充 `depth`/`prev_events`/`auth_events` (本地起源 PDU 恒 `MissingGraphMetadata`)
3. ❌ 无 ED25519-only 强制验证
4. ❌ 无 MSC4311 合规性检查

### 1.2 动画缩略图

**上游实现** (Synapse Python):
- **检测机制**: GIF/PNG/WebP 三格式支持，通过 `is_animated` 属性判断
- **请求参数**: `animated=true/false` (默认 false)
- **输出格式**: 
  - 静态：JPEG/PNG/WebP
  - 动画：**始终 WebP** (`ANIMATED_THUMBNAIL_TYPE = "image/webp"`)
- **帧处理**: 逐帧缩放/裁剪，保留帧延迟 (`duration`) 和循环次数 (`loop`)
- **降级策略**: 动画解码失败 → 自动回退到首帧静态缩略图

**我们项目现状**:
```rust
// synapse-web/src/routes/media/download.rs
pub(crate) fn thumbnail_request_params(params: &Value) -> (u32, u32, &str) {
    // ❌ 没有 animated 参数解析
}

// synapse-services/src/media_service.rs
fn generate_thumbnail(...) -> Result<Vec<u8>, ApiError> {
    // ❌ 总是输出 JPEG
    // ❌ 没有动画检测
    // ❌ 没有帧处理
}
```

**差距**:
1. ❌ 无 `animated` 查询参数支持
2. ❌ 无 GIF/APNG/WebP 动画检测
3. ❌ 无逐帧处理逻辑
4. ❌ 无 WebP 动画编码能力

---

## 二、详细实现方案

### 2.1 v12 房间版本实现路线

#### 阶段 1: 启用 v12 创建 (预计 2-3 周)

**步骤 1.1**: 实现 v12 事件验证

**文件修改清单**:
1. `synapse-storage/src/event/create.rs` - 添加 depth/prev_events/auth_events 计算
2. `synapse-services/src/auth/event_auth.rs` - 添加 ED25519-only 验证
3. `synapse-common/src/event.rs` - 确保事件序列化包含所有 PDU 字段

**关键技术点**:

```rust
// 1. Depth 计算 (基于事件图)
// Depth = max(prev_event_depths) + 1
// 必须查询事件图存储获取 prev events 的 depth
async fn calculate_event_depth(
    &self,
    room_id: &str,
    prev_events: &[EventReference],
) -> Result<i64, ApiError> {
    let max_depth = self.get_prev_events_max_depth(room_id, prev_events).await?;
    Ok(max_depth + 1)
}

// 2. Auth Events 构造 (v12 要求)
// auth_events 必须包含:
// - m.room.create (必需，MSC4311)
// - m.room.power_levels
// - m.room.member (creator)
// - m.room.history_visibility
fn construct_auth_events(
    room_state: &RoomState,
    user_id: &str,
) -> Vec<EventReference> {
    vec![
        room_state.get_event_ref("m.room.create", ""),
        room_state.get_event_ref("m.room.power_levels", ""),
        room_state.get_event_ref("m.room.member", user_id),
        room_state.get_event_ref("m.room.history_visibility", ""),
    ]
}

// 3. 签名验证 (ED25519-only)
// 拒绝非 ED25519 签名算法的事件
fn validate_signatures_v12(event: &Event) -> Result<(), ApiError> {
    for (server_name, signature) in event.signatures.iter() {
        match signature.algorithm() {
            SignatureAlgorithm::Ed25519 => continue,
            _ => return Err(ApiError::forbidden(format!(
                "v12 rooms only support Ed25519 signatures, got {}",
                signature.algorithm()
            ))),
        }
    }
    Ok(())
}
```

**步骤 1.2**: 更新房间版本能力

```rust
// synapse-common/src/room_versions.rs
RoomVersionCapability::stable_parse_only("12")  // OLD
RoomVersionCapability::stable("12")             // NEW
```

**步骤 1.3**: 添加 v12 测试用例

```rust
#[test]
fn test_v12_room_creation() {
    // 创建 v12 房间
    // 验证 PDU 包含有效的 depth, prev_events, auth_events
    // 验证 ED25519-only 签名强制
    // 验证 MSC4311 合规性：m.room.create 在 stripped state 中
}
```

**验收标准**:
- ✅ 可通过 `POST /_matrix/client/v3/createRoom` 创建 v12 房间
- ✅ 创建的 PDU 包含有效的 `depth`、`prev_events`、`auth_events`
- ✅ 非 ED25519 签名被拒绝
- ✅ MSC4311 合规性验证通过

#### 阶段 2: 升级默认版本为 v12 (预计 1 周)

**步骤 2.1**: 修改默认房间版本

```rust
// synapse-common/src/room_versions.rs
pub const DEFAULT_ROOM_VERSION: &str = "12";  // 原来是 "11"
```

**步骤 2.2**: 更新文档

- `docs/synapse-rust-vs-synapse-comparison.md` - 更新房间版本对比
- `docs/audit/*.md` - 记录 v12 能力

**步骤 2.3**: 更新测试

```rust
#[test]
fn test_default_room_version_is_12() {
    assert_eq!(DEFAULT_ROOM_VERSION, "12");
}
```

**回滚计划**: 如果出现问题，将常量改回 "11" 即可。

#### 阶段 3: 安全审计 (预计 1-2 周)

**重点领域**:
1. CVE-2025-49090 缓解措施 (细节未公开，依赖上游指导)
2. ED25519-only 强制执行的完整性
3. PDU 格式联邦兼容性

**验证方法**:
- 与上游 Synapse v1.162+ 实现交叉核对
- 与上游 Synapse 服务器进行联邦测试
- 事件签名验证测试

### 2.2 动画缩略图实现路线

#### 阶段 1: 添加 animated 参数支持 (预计 1-2 天)

**步骤 1.1**: 更新请求解析

**文件**: `synapse-web/src/routes/media/download.rs`

```rust
pub(crate) fn thumbnail_request_params(params: &Value) -> (u32, u32, &str, bool) {
    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(600) as u32;
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or("scale");
    let animated = params.get("animated").and_then(|v| v.as_bool()).unwrap_or(false);
    (width, height, method, animated)
}
```

**步骤 1.2**: 传递到服务层

**文件**: `synapse-services/src/media_service.rs`

```rust
pub async fn get_thumbnail(
    &self,
    _server_name: &str,
    media_id: &str,
    width: u32,
    height: u32,
    method: &str,
    animated: bool,  // 新增参数
) -> Result<Vec<u8>, ApiError>
```

#### 阶段 2: 实现动画检测和生成 (预计 1-2 周)

**依赖检查**: 需要确认 `image` crate 是否支持动画迭代

**步骤 2.1**: 添加动画检测

```rust
use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use std::io::Cursor;

fn is_animated_image(data: &[u8]) -> bool {
    // 检查 GIF
    if let Ok(mut decoder) = GifDecoder::new(Cursor::new(data)) {
        if let Ok(num_frames) = decoder.num_frames() {
            return num_frames > 1;
        }
    }
    // 检查 WebP
    if let Ok(mut decoder) = WebPDecoder::new(Cursor::new(data)) {
        if let Ok(total_frames) = decoder.total_frames() {
            return total_frames > 1;
        }
    }
    // 检查 APNG (通过 PNG 解码器)
    // TODO: 实现 APNG 检测
    false
}
```

**步骤 2.2**: 实现帧处理

```rust
fn generate_thumbnail(
    image_data: &[u8],
    target_width: u32,
    target_height: u32,
    method: ThumbnailMethod,
    animated: bool,
) -> Result<Vec<u8>, ApiError> {
    // 检查源图是否动画
    let source_animated = is_animated_image(image_data);
    
    if animated && source_animated {
        // 生成动画 WebP 缩略图
        generate_animated_thumbnail(image_data, target_width, target_height, method)
    } else {
        // 生成静态缩略图 (现有逻辑)
        generate_static_thumbnail(image_data, target_width, target_height, method)
    }
}
```

**步骤 2.3**: 动画 WebP 编码

```rust
fn generate_animated_thumbnail(
    data: &[u8],
    width: u32,
    height: u32,
    method: ThumbnailMethod,
) -> Result<Vec<u8>, ApiError> {
    use image::codecs::webp::WebPEncoder;
    use image::AnimationDecoder;
    
    // 解码所有帧
    let frames = /* 迭代帧 */;
    
    // 处理每帧 (缩放/裁剪)
    let processed_frames: Vec<DynamicImage> = frames.map(|frame| {
        let transformed = /* 应用变换 */;
        transformed
    }).collect();
    
    // 编码为带动画的 WebP
    let mut output = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut output);
    // 配置动画设置
    // ...
    
    Ok(output)
}
```

#### 阶段 3: 测试和验证 (预计 1 周)

**测试用例**:

1. **静态图像**:
   - JPEG 输入 → JPEG 输出 (行为不变)
   - PNG 输入 → JPEG 输出 (行为不变)

2. **动画 GIF**:
   - `animated=false` → 静态 JPEG (第一帧)
   - `animated=true` → 动画 WebP

3. **动画 WebP**:
   - `animated=false` → 静态 JPEG (第一帧)
   - `animated=true` → 动画 WebP

4. **APNG**:
   - `animated=false` → 静态 JPEG (第一帧)
   - `animated=true` → 动画 WebP

5. **降级行为**:
   - 损坏的动画 → 降级到静态
   - 不支持的格式 → 返回原图或错误

**验证命令**:

```bash
# 测试静态缩略图 (现有)
curl "http://localhost:9090/_matrix/media/v3/thumbnail/example.com/abc123?width=200&height=200&method=scale"

# 测试动画缩略图 (新)
curl "http://localhost:9090/_matrix/media/v3/thumbnail/example.com/abc123?width=200&height=200&method=scale&animated=true"

# 验证响应 Content-Type
# 预期：动画为 image/webp，静态为 image/jpeg
```

---

## 三、风险评估

### 3.1 v12 房间版本风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 联邦不兼容 | 高 | 在启用前与上游 Synapse 进行测试 |
| 事件验证失败 | 中 | 逐步推出：parse-only → create-enabled |
| 签名验证 bug | 高 | 广泛的加密测试，mutation self-proof |
| MSC4311 合规性问题 | 中 | 紧密跟随上游实现 |

### 3.2 动画缩略图风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| CPU 耗尽 (帧处理) | 高 | 速率限制，超时控制 |
| 内存耗尽 (大动画) | 高 | 帧数限制，内存上限 |
| 磁盘耗尽 (缓存动画缩略图) | 中 | 缓存大小限制，TTL 淘汰 |
| WebP 编码失败 | 低 | 降级到静态，错误日志 |

---

## 四、依赖关系

### 外部依赖

1. **上游 Synapse**: v12 认证规则的参考实现
2. **image crate**: 动画支持 (检查版本兼容性)
3. **WebP 编码**: 确保 `image` crate 支持动画 WebP 输出

### 内部依赖

1. **Track 1 优先于 Track 2**: 房间版本变更影响核心事件处理
2. **Schema 迁移**: 两个轨道都不需要
3. **数据库变更**: 预计不需要

---

## 五、时间估算

### Track 1: v12 房间版本

| 阶段 | 估计时间 | 依赖 |
|------|---------|------|
| 阶段 1: 启用 v12 创建 | 2-3 周 | 事件认证、存储层 |
| 阶段 2: 升级默认版本 | 1 周 | 阶段 1 完成 |
| 阶段 3: 安全审计 | 1-2 周 | 外部审查 |

**总计**: 4-6 周

### Track 2: 动画缩略图

| 阶段 | 估计时间 | 依赖 |
|------|---------|------|
| 阶段 1: 参数支持 | 1-2 天 | 仅 API 层 |
| 阶段 2: 动画逻辑 | 1-2 周 | image crate 功能 |
| 阶段 3: 测试 | 1 周 | 无 |

**总计**: 2-3 周

---

## 六、下一步行动

### 立即可执行

1. **确定优先级**: 决定先处理哪个轨道 (推荐：Track 1)
2. **分配负责人**: 为每个轨道指定主导工程师
3. **搭建测试环境**: 准备与上游 Synapse 的联邦测试环境

### 待审核事项

- [ ] 批准 v12 认证规则的实现方案
- [ ] 确认 image crate 版本支持动画 WebP
- [ ] 审查 v12 升级的安全影响
- [ ] 安排联邦兼容性测试

### 文档更新

实现后需要更新:

1. `docs/audit/sdk-encapsulation-audit.md` - 记录 v12 能力
2. `docs/audit/animated-thumbnail-implementation.md` - 记录实现细节
3. `docs/synapse-rust-vs-synapse-comparison.md` - 更新房间版本对齐情况

---

## 七、Mutation Self-Proof 测试用例

### v12 房间版本

```rust
#[test]
fn test_v12_rejects_non_ed25519_signatures() {
    // 创建带有 RSA 签名的事件 → 应该验证失败
    // 验证拒绝
}

#[test]
fn test_v12_requires_m_room_create_in_auth() {
    // 创建缺少 m.room.create 在 auth_events 中的事件 → 应该失败
    // 验证拒绝
}

#[test]
fn test_v12_creates_valid_pdu() {
    // 创建 v12 房间
    // 验证 PDU 包含有效的 depth, prev_events, auth_events
    // 验证上游 Synapse 接受来自本服务器的 PDU
}
```

### 动画缩略图

```rust
#[test]
fn test_animated_gif_generates_webp() {
    // 上传动画 GIF
    // 使用 animated=true 请求缩略图
    // 验证响应是包含多帧的 WebP
}

#[test]
fn test_animated_fallback_on_error() {
    // 上传损坏的动画
    // 使用 animated=true 请求缩略图
    // 验证降级到静态 JPEG
}
```

---

**文档结束**
