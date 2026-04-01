# 德文（de）本地化与显示质量审查

## 当前现状

- 本仓库为 Matrix homeserver 后端实现，未发现独立前端 UI 工程与翻译资源包（无 locales/i18n 目录，无 .po/.mo/.ftl/.arb/.properties 等资源）。
- 对外文本主要来自 API 错误文案与少量 HTML/XML 字符串响应（例如 CAS logout、SAML SP metadata）。

## 已落实的最小改进（显示异常优先）

- 对 `text/html` 与 `application/xml` 响应补齐 `charset=utf-8`，降低德文等非 ASCII 字符在浏览器/代理链路中的乱码风险。
- 增加编码检查脚本：`bash scripts/quality/check_text_encoding.sh`，在证据采集时自动执行。

## 达成“德文本地化完整度 100%”的缺口

- 缺失集中式 i18n 机制：没有“文本键→多语言资源→按 Accept-Language 选择”的基础设施。
- 缺失 UI 回归载体：没有可截图验证的 UI 页面集合（除少量 SSO 辅助页），无法定义“关键页面”与稳定截图基线。

## 建议路线（后续任务）

- 明确目标范围：
  - 若目标是“服务器错误码/错误描述多语言化”，需为 `ApiError` 与用户可见消息引入 i18n 表与语言协商。
  - 若目标是“管理控制台/用户 UI 德文完整度”，需先引入 UI 工程或对接现有客户端页面。
- 先落地可验证门禁：
  - i18n 键完整度检查（de 必须覆盖 en 的键集合）。
  - 关键响应（HTML/XML/JSON）编码与 Content-Type 合规检查。

