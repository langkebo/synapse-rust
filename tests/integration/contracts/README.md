# integration/contracts/

本目录用于承载“路由契约/错误语义稳定性”测试：
- 关注 URL + method + 标准错误码（Matrix errcode）+ 关键响应字段。
- 禁止只断言 HTTP status；若返回 `{}`，必须验证真实副作用或可回读状态。
- 新增 contract 测试优先落在该目录对应能力域子目录中。

