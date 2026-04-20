import json
from pathlib import Path

def load_results(role):
    path = Path(f"test-results/{role}/api-integration.responses.jsonl")
    if not path.exists():
        return []
    results = []
    with open(path, "r") as f:
        for line in f:
            if line.strip():
                results.append(json.loads(line))
    return results

def analyze():
    roles = ["super_admin", "admin", "user"]
    data = {}
    
    for role in roles:
        results = load_results(role)
        for res in results:
            case = res["case"]
            if case not in data:
                data[case] = {}
            data[case][role] = {
                "outcome": res["outcome"],
                "status": res["http_status"],
                "reason": res.get("reason", "")
            }
    
    # Identify issues
    vulnerabilities = []
    failures = []
    missing_features = []
    
    for case, roles_data in data.items():
        for role in ["admin", "user"]:
            if roles_data.get(role, {}).get("outcome") == "fail" and "SECURITY VULNERABILITY" in roles_data.get(role, {}).get("reason", ""):
                vulnerabilities.append({
                    "case": case,
                    "role": role,
                    "reason": roles_data[role]["reason"]
                })
        
        if roles_data.get("super_admin", {}).get("outcome") == "fail":
            failures.append({
                "case": case,
                "role": "super_admin",
                "reason": roles_data["super_admin"]["reason"]
            })
            
        if roles_data.get("super_admin", {}).get("outcome") == "missing" or "M_UNRECOGNIZED" in roles_data.get("super_admin", {}).get("reason", ""):
            missing_features.append(case)

    # Generate CSV (manual)
    with open("docs/synapse-rust/permission_matrix.csv", "w") as f:
        f.write("Case,super_admin_status,super_admin_outcome,admin_status,admin_outcome,user_status,user_outcome\n")
        for case, roles_data in data.items():
            row = [case]
            for role in roles:
                d = roles_data.get(role, {"outcome": "N/A", "status": "N/A"})
                row.append(str(d["status"]))
                row.append(str(d["outcome"]))
            f.write(",".join(row) + "\n")

    # Generate Report
    report = f"""# API 权限安全测试与功能完整性验证报告

## 1. 测试概览
- **测试时间**: 2026-04-19
- **测试环境**: Docker (http://localhost:28008)
- **测试角色**: 超级管理员 (super_admin), 管理员 (admin), 普通用户 (user)
- **测试工具**: `api-integration_test.sh`

## 2. 权限对比矩阵 (关键摘录)
详细矩阵见 [permission_matrix.csv](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/permission_matrix.csv)

| 功能 | super_admin | admin | user |
| --- | --- | --- | --- |
"""
    sample_cases = ["Health endpoint", "Admin List Users", "Admin User Details", "Create Test Room", "Admin User Deactivate"]
    for sc in sample_cases:
        if sc in data:
            row = f"| {sc} | {data[sc].get('super_admin', {}).get('status', 'N/A')} | {data[sc].get('admin', {}).get('status', 'N/A')} | {data[sc].get('user', {}).get('status', 'N/A')} |\n"
            report += row

    report += f"""
## 3. 安全风险 (垂直越权)
以下接口在较低权限下被允许访问，存在安全风险：
"""
    if vulnerabilities:
        for v in vulnerabilities:
            report += f"- **{v['case']}**: 角色 `{v['role']}` 越权访问. 原因: {v['reason']}\n"
    else:
        report += "- 未发现明显的垂直越权漏洞。\n"

    report += """
## 4. 功能性缺陷 (失败用例)
"""
    if failures:
        for f in failures:
            report += f"- **{f['case']}**: {f['reason']}\n"
    else:
        report += "- 超级管理员角色下核心功能测试全部通过。\n"

    report += """
## 5. 缺失功能 (未实现或返回 M_UNRECOGNIZED)
"""
    if missing_features:
        for m in missing_features:
            report += f"- {m}\n"
    else:
        report += "- 未发现明显的缺失功能。\n"

    report += """
## 6. 优化方案与建议
### 6.1 权限控制修复
1. **统一鉴权中间件**: 确保所有 `/admin` 路径的接口都经过严格的权限检查。
2. **细粒度权限校验**: 在业务逻辑层增加对 `is_admin` 和 `is_super_admin` 的区分。

### 6.2 功能补全计划
1. **补全缺失 API**: 针对返回 `M_UNRECOGNIZED` 的接口，排期实现。
2. **完善错误处理**: 统一返回格式，避免敏感信息泄露。

### 6.3 回归测试计划
1. 在修复越权问题后，重新运行全量测试。
2. 增加自动化回归脚本到 CI 流程。
"""

    with open("docs/synapse-rust/API_SECURITY_VERIFICATION_REPORT.md", "w") as f:
        f.write(report)
    print("Report generated: docs/synapse-rust/API_SECURITY_VERIFICATION_REPORT.md")

if __name__ == "__main__":
    analyze()
