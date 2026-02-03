import os
import re

PROJECT_ROOT = "/home/hula/synapse_rust"
SRC_DIR = os.path.join(PROJECT_ROOT, "src")
ROUTES_DIR = os.path.join(SRC_DIR, "web", "routes")

def get_balanced_content(text, start_index):
    count = 0
    for i in range(start_index, len(text)):
        if text[i] == '(':
            count += 1
        elif text[i] == ')':
            count -= 1
            if count == 0:
                return text[start_index+1:i], i
    return None, -1

def scan_file(filename):
    filepath = os.path.join(ROUTES_DIR, filename)
    if not os.path.exists(filepath):
        return []
        
    with open(filepath, 'r') as f:
        content = f.read()
    
    protected_handlers = []
    if filename == "federation.rs":
        match = re.search(r'let protected = Router::new\(\)(.*?)\.layer\(', content, re.DOTALL)
        if match:
            block = match.group(1)
            protected_handlers = re.findall(r'\((?P<handler>\w+)\)', block)

    apis = []
    start_pos = 0
    while True:
        idx = content.find(".route(", start_pos)
        if idx == -1:
            break
        
        inner_content, end_idx = get_balanced_content(content, idx + 6)
        start_pos = end_idx + 1
        
        if not inner_content:
            continue
            
        parts = inner_content.split(',', 1)
        if len(parts) < 2:
            continue
            
        path = parts[0].strip().strip('"')
        methods_block = parts[1].strip()
        
        method_matches = re.finditer(r'(?P<method>get|post|put|delete)\s*\(\s*(?P<handler>.*?)\s*\)', methods_block, re.DOTALL)
        
        for m_match in method_matches:
            method = m_match.group('method').upper()
            handler_raw = m_match.group('handler').strip()
            
            if "||" in handler_raw or "async" in handler_raw:
                handler = "inline closure"
                auth = "None"
            else:
                handler = handler_raw.split('(')[0].strip()
                auth = "None"
                handler_found = False
                for r_file in os.listdir(ROUTES_DIR):
                    with open(os.path.join(ROUTES_DIR, r_file), 'r') as hf:
                        h_content = hf.read()
                        sig_start = h_content.find(f"async fn {handler}")
                        if sig_start != -1:
                            args = get_args(h_content, sig_start + len(f"async fn {handler}"))
                            if args:
                                if "AdminUser" in args: auth = "Matrix (Admin)"
                                elif "AuthenticatedUser" in args: auth = "Matrix (User)"
                                elif "federation_auth" in args: auth = "Federation"
                                elif "headers" in args: auth = "Matrix (Manual Token)"
                                handler_found = True
                                break
                
                if not handler_found:
                    if filename == "federation.rs" and handler in protected_handlers:
                        auth = "Federation"
                    elif filename == "admin.rs":
                        auth = "Matrix (Admin)"

            apis.append({
                "path": path,
                "method": method,
                "handler": handler,
                "auth": auth,
                "module": filename.replace(".rs", "")
            })
            
    return apis

def get_args(content, start_index):
    idx = content.find('(', start_index)
    if idx == -1: return None
    args, _ = get_balanced_content(content, idx)
    return args

def main():
    files = [
        "mod.rs", "admin.rs", "e2ee_routes.rs", "federation.rs", 
        "friend.rs", "key_backup.rs", "media.rs", "private_chat.rs", "voice.rs"
    ]
    
    all_apis = []
    for f in files:
        all_apis.extend(scan_file(f))
    
    unique_apis = {}
    for api in all_apis:
        key = (api['path'], api['method'], api['module'])
        if key not in unique_apis:
            unique_apis[key] = api
            
    sorted_apis = sorted(unique_apis.values(), key=lambda x: x['path'])
    
    md_path = os.path.join(PROJECT_ROOT, "docs", "synapse-rust", "api-reference.md")
    with open(md_path, 'w') as f:
        f.write("# Synapse Rust API 完整清单 (Full API Inventory)\n\n")
        f.write(f"> **已识别接口总数**: {len(sorted_apis)}\n")
        f.write("> **扫描日期**: 2026-02-02\n")
        f.write("> **系统版本**: 0.1.0-alpha\n\n")
        
        f.write("## 1. 业务统计仪表板\n\n")
        f.write("| 统计项 | 数值 |\n")
        f.write("| :--- | :--- |\n")
        f.write(f"| 总接口数 | {len(sorted_apis)} |\n")
        
        f.write("\n## 2. API 接口全量档案\n\n")
        f.write("| 方法 | 完整路径 | 鉴权要求 | 业务模块 | 处理函数 | 测试状态 | 备注 |\n")
        f.write("| :--- | :--- | :--- | :--- | :--- | :--- | :--- |\n")
        for api in sorted_apis:
            f.write(f"| {api['method']} | `{api['path']}` | **{api['auth']}** | {api['module']} | `{api['handler']}` | ⏳ Pending | - |\n")

if __name__ == "__main__":
    main()
