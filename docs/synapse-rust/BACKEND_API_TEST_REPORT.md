# 后端 API 测试报告

**测试日期**: 2026-03-15  
**测试方式**: 直接 HTTP 请求测试真实后端

---

## 测试结果汇总

### ✅ 正常工作的 API (13/16)

| API | 状态 | 说明 |
|-----|------|------|
| Sync | ✅ OK | |
| Create Room | ✅ OK | |
| Get Room State | ✅ OK | |
| Get Room Members | ✅ OK | |
| Send Message | ✅ OK | |
| Typing | ✅ OK | |
| Profile | ✅ OK | |
| Presence | ✅ OK | |
| Devices | ✅ OK | |
| Push Rules | ✅ OK | |
| Account Data | ✅ OK | |
| Search | ✅ OK | |
| Keys Upload | ✅ OK | |

### ⚠️ 发现的问题 (3)

#### 1. Read Receipt - M_NOT_FOUND

**原因**: 测试时使用的事件 ID 不存在

**状态**: 🔧 不是 bug，是测试数据问题

#### 2. Room Tags - 需要数字类型

**问题**: 后端期望 `order` 为 `f64` 数字类型

**SDK 状态**: ✅ SDK 代码正确，发送的是 number

**测试结果**: 
- 发送数字 `{"order": 1}` → 200 OK
- 发送字符串 `{"order": "1"}` → 422 Error

**状态**: ✅ 已验证正常工作

#### 3. VoIP Turn Server - M_NOT_FOUND

**原因**: TURN 服务器未配置

**状态**: ⚠️ 需要配置 TURN 服务器

---

## 测试命令

```bash
# 运行综合测试
node /tmp/final_test.mjs
```

---

## 结论

后端 API 基本完整可用，核心功能正常工作。
