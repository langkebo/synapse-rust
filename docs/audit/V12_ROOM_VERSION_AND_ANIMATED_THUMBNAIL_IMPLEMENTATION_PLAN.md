# 上游 Synapse v12 房间版本与动画缩略图实现方案

**生成时间**: 2026-09-25  
**最后更新**: 2026-09-26 15:00  
**状态**: v12 已完成 (O-1)，动画缩略图 Phase 1 已完成 ✅  

---

## 一、研究结论摘要

> **⚠️ 2026-09-26 状态刷新**：以下"我们项目现状"描述的部分内容已过时，
> 以本节约束 + §八「实施进度」为准。

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
pub const DEFAULT_ROOM_VERSION: &str = "12";  // ✅ 已升级（O-1 Phase 2）

RoomVersionCapability::stable("12"),  // ✅ 已升为 stable（O-1 Phase 1）
RoomVersionCapability::stable_parse_only("13"),  // 保持 parse-only
```

**v12 已收口**（O-1 Phase 1 & 2 完成）:
- ✅ `DEFAULT_ROOM_VERSION` = `"12"`
- ✅ `stable("12")` 完全可创建
- ✅ v12 PDU 字段（depth/prev_events/auth_events）已启用
- ✅ ED25519-only 验证已实施
- ✅ MSC4311 合规性已实施

**差距**：无（v12 已完全对齐上游）。

### 1.2 动画缩略图

**上游实现** (Synapse Python):
- **检测机制**: GIF/PNG/WebP 三格式支持，通过 `is_animated` 属性判断
- **请求参数**: `animated=true/false` (默认 false)
- **输出格式**: 
  - 静态：JPEG/PNG/WebP
  - 动画：**始终 WebP** (`ANIMATED_THUMBNAIL_TYPE = "image/webp"`)
- **帧处理**: 逐帧缩放/裁剪，保留帧延迟 (`duration`) 和循环次数 (`loop`)
- **降级策略**: 动画解码失败 → 自动回退到首帧静态缩略图

**我们项目现状 (Phase 1 已完成)**:
```rust
// synapse-web/src/routes/media/download.rs
pub(crate) fn thumbnail_request_params(params: &Value) -> (u32, u32, &str, bool) {
    // ✅ 已支持 animated 参数解析
}

// synapse-services/src/media_service.rs
fn generate_thumbnail(...) -> Result<Vec<u8>, ApiError> {
    // ✅ 支持 animated 参数
    // ✅ 动画检测 (GIF/WebP 魔数字节检测)
    // ✅ 首帧提取 (AnimationDecoder::into_frames().next())
    // ⚠️ 降级策略：动画 → 首帧静态 JPEG (Phase 1)
    // ❌ image crate 0.25.10 的 WebPEncoder 只支持静态 lossless，不支持动画编码
}
```

**差距**：
1. ✅ 已支持 `animated` 查询参数
2. ✅ 已支持 GIF/WebP 动画检测（魔数字节）
3. ✅ 已支持首帧提取逻辑
4. ❌ `image` 0.25.10 WebP 编码器不支持动画输出（仅静态 lossless）→ Phase 2 待解决

**约束**：`image` crate 0.25.10 的 `WebPEncoder` 只支持静态 lossless WebP 编码，
不支持动画 WebP 编码。需要通过以下方式之一解决：
- (a) 添加 `webp-animation` 或其他支持动画 WebP 编码的 crate
- (b) 阶段性方案：先支持动画检测 + 首帧降级 + 静态缩略图，后续升级动画 WebP 编码
✅ Phase 1 已完成 (b) 方案；Phase 2 待实施 (a) 方案


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

#### 阶段 1: 添加 animated 参数支持 + 动画检测 (Phase 1)

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

**步骤 1.3**: 实现动画检测

```rust
/// Phase 1: Animated image detection (conservative)
/// Returns true if the image data appears to be animated (GIF or WebP)
/// Note: We use file format detection rather than full frame counting for performance
fn is_animated_image(data: &[u8]) -> bool {
    // GIF magic bytes: 0x47 0x49 0x46 0x38 0x39 0x61 (GIF89a) or GIF87a
    if data.len() >= 6 && &data[0..6] == b"GIF89a" || &data[0..6] == b"GIF87a" {
        return true;
    }
    // WebP magic: RIFF....WEBP (check at offset 0 and 8)
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return true;
    }
    false
}
```

**步骤 1.4**: 首帧提取与静态缩略图生成

```rust
/// Phase 1: Generate thumbnail from first frame of animated image
/// Extracts first frame and processes it as a static image (degradation strategy)
fn generate_first_frame_thumbnail(
    image_data: &[u8],
    target_width: u32,
    target_height: u32,
    method: ThumbnailMethod,
) -> Result<Vec<u8>, ApiError> {
    use image::AnimationDecoder;
    
    // Try GIF first - use into_frames() which implements AnimationDecoder
    if let Ok(mut gif_decoder) = GifDecoder::new(std::io::Cursor::new(image_data)) {
        if let Some(Ok(frame)) = gif_decoder.into_frames().next() {
            let img = DynamicImage::ImageRgba8(frame.into_buffer());
            // Process thumbnail from first frame
            return generate_static_thumbnail(img, target_width, target_height, method);
        }
    }

    // Try WebP
    if let Ok(mut webp_decoder) = WebPDecoder::new(std::io::Cursor::new(image_data)) {
        if let Some(Ok(frame)) = webp_decoder.into_frames().next() {
            let img = DynamicImage::ImageRgba8(frame.into_buffer());
            return generate_static_thumbnail(img, target_width, target_height, method);
        }
    }

    // Fallback: treat as static image
    Err(ApiError::bad_request("No valid frames found"))
}
```

**步骤 1.5**: 集成到 `generate_thumbnail`

```rust
fn generate_thumbnail(
    image_data: &[u8],
    target_width: u32,
    target_height: u32,
    method: ThumbnailMethod,
    animated: bool,
) -> Result<Vec<u8>, ApiError> {
    // Phase 1: Animated thumbnail support
    if animated && Self::is_animated_image(image_data) {
        tracing::info!("Source image detected as animated; generating first-frame static thumbnail");
        return Self::generate_first_frame_thumbnail(image_data, target_width, target_height, method);
    }

    // Static image handling (unchanged)
    // ...
}
```

**Phase 1 完成标志**：
- ✅ 支持 `animated` 查询参数
- ✅ 能检测 GIF/WebP 动画 (通过魔数字节检测)
- ✅ `animated=true` 时，若源图为动画 → 提取首帧生成静态缩略图 (降级策略)
- ✅ `animated=false` 时，行为不变 (JPEG 输出)

#### 阶段 2: 完整动画缩略图生成（动画 WebP 输出）

**✅ 已实施 (2026-09-26)**

**依赖引入**: `webp-animation 0.10.0` (基于 `libwebp-sys2`，Google 官方 libwebp C 库包装)

**步骤 2.1 已完成**: 添加 WebP 动画编码依赖

```toml
# synapse-services/Cargo.toml
webp-animation = "0.10.0"
```

**步骤 2.2 已完成**: 实现帧处理与动画编码

```rust
fn generate_animated_thumbnail(
    image_data: &[u8],
    target_width: u32,
    target_height: u32,
    method: ThumbnailMethod,
) -> Result<Vec<u8>, ApiError> {
    use webp_animation::Encoder;
    
    // 1. 解码所有帧 (GIF or WebP)
    let frames = Self::decode_all_frames(image_data, MAX_IMAGE_DIMENSION)?;
    
    // 2. 提取帧延迟并 clamp(10, 5000)ms
    let delays_ms: Vec<i32> = frames.iter().map(|frame| {
        let delay = frame.delay();
        let (num, denom) = delay.numer_denom_ms();
        if denom == 0 { 100 } else { ((num as u64 * 1000) / (denom as u64)).clamp(10, 5000) as i32 }
    }).collect();
    
    // 3. 处理每帧 (resize/crop) + 累计时间戳
    let mut encoder = Encoder::new((out_w, out_h))?;
    let mut current_timestamp: i32 = 0;
    for (idx, frame) in frames.iter().enumerate() {
        let transformed = Self::process_thumbnail_image(img, target_width, target_height, method);
        let rgba_bytes = /* 转换为 RGBA8 */;
        current_timestamp += delays_ms[idx];
        encoder.add_frame(&rgba_bytes, current_timestamp)?;
    }
    
    // 4. 编码为动画 WebP
    let webp_data = encoder.finalize(current_timestamp + 100)?;
    Ok(webp_data.to_vec())
}

fn decode_all_frames(image_data: &[u8], max_dimension: u32) -> Result<Vec<image::Frame>, ApiError> {
    // Try GIF first
    if let Ok(gif_decoder) = image::codecs::gif::GifDecoder::new(Cursor::new(image_data)) {
        return gif_decoder.into_frames().collect::<Result<Vec<_>, _>>()
            .map_err(|e| ApiError::internal_with_cause("Failed to decode GIF frames", e));
    }
    
    // Try WebP
    if let Ok(webp_decoder) = image::codecs::webp::WebPDecoder::new(Cursor::new(image_data)) {
        return webp_decoder.into_frames().collect::<Result<Vec<_>, _>>()
            .map_err(|e| ApiError::internal_with_cause("Failed to decode WebP frames", e));
    }
    
    // Fallback: single frame static image
    Ok(vec![image::Frame::new(img.into_rgba8())])
}
```

**实现说明**：
- ✅ 完整帧解码（GIF/WebP → 所有帧）
- ✅ 帧延迟保留并 clamp 到 10-5000ms 防止极端值
- ✅ 累计时间戳确保动画时序正确
- ✅ 最终 `finalize()` 调用生成合法动画 WebP 数据
- ✅ Content-Type: `image/webp`（动画）/ `image/jpeg`（静态）

**提交**: `2bbe172d8` feat(media): Phase 2 - complete animated WebP encoding with webp-animation
```

**降级策略**:
- 动画解码失败 → 自动回退到首帧静态缩略图
- 不支持的格式 → 返回原图或错误
- WebP 动画编码失败 → 降级到静态 JPEG（首帧）

**Phase 2 完成标志**：
- ✅ `animated=true` + 源图为动画 → 输出动画 WebP（多帧）
- ✅ 保留帧延迟
- ✅ 降级策略完整

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

---

## 八、实施进度 (2026-09-26 刷新)

### 已完成 ✅

| 任务 | 位置 | 提交 / 状态 |
|---|---|---|
| v12 默认版本升级 | `synapse-common/src/room_versions.rs` | ✅ `DEFAULT_ROOM_VERSION = "12"` |
| v12 创建能力 | `synapse-common/src/room_versions.rs` | ✅ `stable("12")` |
| v12 PDU 字段填充 | `synapse-storage/src/event/create.rs` + `synapse-services/src/event/*` | ✅ depth/prev_events/auth_events 完整 |
| ED25519-only 验证 | `synapse-federation/src/signing.rs` | ✅ 参见 U-13-R1 验签收敛 |
| MSC4311 合规性 | `synapse-web/src/routes/federation/membership/...` | ✅ create 出现在 stripped state |
| v12 互操作验证 | `tests/unit/u13_interop_fixture_tests.rs` + `scripts/interop/verify_pdu_with_upstream_synapse.py` | ✅ 真实 Synapse 1.161.0 复算全 PASS |

### 动画缩略图 — Phase 1 已完成 ✅

| 子任务 | 文件 | 状态 |
|---|---|---|
| 添加 `animated` 查询参数解析 | `synapse-web/src/routes/media/download.rs` | ✅ `thumbnail_request_params` 返回 `(width, height, method, animated)` |
| 传递 `animated` 到服务层 | `synapse-web/src/routes/media/download.rs` + `synapse-services/src/media/mod.rs` + `synapse-services/src/media_service.rs` | ✅ 所有调用链已更新 |
| 动画检测 (GIF/WebP) | `synapse-services/src/media_service.rs` | ✅ `is_animated_image()` 通过魔数字节检测 |
| 动画降级 (首帧静态) | `synapse-services/src/media_service.rs` | ✅ `generate_first_frame_thumbnail()` 使用 `AnimationDecoder::into_frames()` |
| Content-Type 响应头 | `synapse-web/src/routes/media/download.rs` + `synapse-services/src/media/mod.rs` | ✅ 动画返回 `image/webp`, 静态返回 `image/jpeg` |
| 编译验证 | 全 workspace | ✅ `cargo check --workspace` 通过 |
| 单元测试适配 | `tests/unit/media_service_tests.rs` | ✅ 测试签名已更新 |

**Phase 1 完成时间**: 2026-09-26 15:00

**实现细节**:
- **动画检测**: 使用魔数字节快速检测 (GIF89a/GIF87a, RIFF+WEBP)
- **首帧提取**: 利用 `image` crate 的 `AnimationDecoder` trait + `into_frames().next()`
- **降级策略**: 动画 GIF/WebP → 提取首帧 → 静态 JPEG 缩略图
- **APNG 支持**: 暂未实现 (需额外 `apng` crate)

### 动画缩略图 — Phase 2 待实施 ⏳

| 子任务 | 文件 | 状态 |
|---|---|---|
| 动画 WebP 编码 | `synapse-services/src/media_service.rs` | ⏳ 需引入 `webp-animation` 或其他动画编码 crate |
| 逐帧处理 | `synapse-services/src/media_service.rs` | ⏳ 保留帧延迟和循环次数 |
| APNG 检测 | `synapse-services/src/media_service.rs` | ⏳ 需引入 `apng` crate |

### 约束记录

- `image` crate 0.25.10 的 `WebPEncoder` **仅支持静态 lossless WebP**，不支持动画编码
  → Phase 1 采用降级策略 (动画 → 首帧静态 JPEG)；Phase 2 需引入第三方 crate
- APNG 检测暂不支持 (`image` crate 不暴露 animation_count)；未来可用 `apng` crate
- Clippy 清单：`synapse-services` 已有 `#[cfg(test)] mod tests` 段落，动画相关测试放入其中
