import os
import re
import json
import datetime
import shutil
import hashlib

# Configuration
PROJECT_ROOT = "/home/hula/synapse_rust"
DOCS_DIR = os.path.join(PROJECT_ROOT, "docs/synapse-rust")
SRC_DIR = os.path.join(PROJECT_ROOT, "src")
BACKUP_DIR = os.path.join(DOCS_DIR, "backup")

# Keywords for TODO extraction
TODO_KEYWORDS = [
    "TODO", "FIXME", "XXX", "HACK", "OPTIMIZE", 
    "待开发", "后续计划", "已知缺陷", "性能瓶颈", "优化"
]
TODO_REGEX = re.compile(r"(\b(" + "|".join(TODO_KEYWORDS) + r")\b.*)", re.IGNORECASE)

# Keywords for priority
PRIORITY_MAP = {
    "CRITICAL": "P0",
    "HIGH": "P1",
    "MED": "P2",
    "LOW": "P3"
}

def get_timestamp():
    return datetime.datetime.now().strftime("%Y%m%d_%H%M%S")

def ensure_dir(path):
    if not os.path.exists(path):
        os.makedirs(path)

def scan_files(directory, extensions):
    matches = []
    for root, dirs, files in os.walk(directory):
        for file in files:
            if any(file.endswith(ext) for ext in extensions):
                matches.append(os.path.join(root, file))
    return matches

def clean_and_standardize_doc(file_path):
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    original_content = content
    
    # 1. Fix headers (e.g. "#Title" -> "# Title")
    content = re.sub(r'^(#+)([^#\s])', r'\1 \2', content, flags=re.MULTILINE)
    
    # 2. Fix code blocks (add 'text' if lang is missing)
    # content = re.sub(r'```\s*\n', '```text\n', content) # Too aggressive, might break existing blocks
    
    # 3. Check if obsolete
    is_obsolete = False
    if "deprecated" in content.lower() or "obsolete" in content.lower():
        # Heuristic: check if it's in the metadata or title
        if re.search(r'^#.*(deprecated|obsolete)', content, re.IGNORECASE | re.MULTILINE):
            is_obsolete = True
            
    if content != original_content:
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(content)
            
    return is_obsolete

def extract_todos(file_path):
    todos = []
    rel_path = os.path.relpath(file_path, PROJECT_ROOT)
    
    try:
        with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
            lines = f.readlines()
            
        for i, line in enumerate(lines):
            match = TODO_REGEX.search(line)
            if match:
                content = match.group(1).strip()
                
                # Determine priority
                priority = "P2" # Default
                for key, val in PRIORITY_MAP.items():
                    if key in content.upper():
                        priority = val
                        break
                if "!!!" in content or "CRITICAL" in content.upper():
                    priority = "P0"
                
                # Determine owner
                owner = ""
                owner_match = re.search(r'@(\w+)', line)
                if owner_match:
                    owner = owner_match.group(1)
                
                todos.append({
                    "file": rel_path,
                    "line": i + 1,
                    "content": content,
                    "priority": priority,
                    "owner": owner,
                    "type": match.group(2).upper()
                })
    except Exception as e:
        print(f"Error reading {file_path}: {e}")
        
    return todos

def extract_implemented_functions(src_dir):
    functions = []
    for root, dirs, files in os.walk(src_dir):
        for file in files:
            if file.endswith(".rs"):
                path = os.path.join(root, file)
                rel_path = os.path.relpath(path, PROJECT_ROOT)
                try:
                    with open(path, 'r', encoding='utf-8') as f:
                        content = f.read()
                        # Simple regex for pub fn
                        matches = re.findall(r'pub\s+fn\s+(\w+)', content)
                        for m in matches:
                            functions.append({
                                "name": m,
                                "file": rel_path
                            })
                except:
                    pass
    return functions

def main():
    ensure_dir(DOCS_DIR)
    ensure_dir(BACKUP_DIR)
    
    timestamp = get_timestamp()
    
    # 1. Scan and Clean Docs
    doc_files = scan_files(DOCS_DIR, ['.md', '.txt'])
    obsolete_files = []
    
    print(f"Scanning {len(doc_files)} documentation files...")
    
    for doc in doc_files:
        if clean_and_standardize_doc(doc):
            obsolete_files.append(doc)
            
    # Handle obsolete files
    if obsolete_files:
        print(f"Found {len(obsolete_files)} obsolete files. Moving to backup...")
        for f in obsolete_files:
            fname = os.path.basename(f)
            shutil.move(f, os.path.join(BACKUP_DIR, fname))
            
    # 2. Extract TODOs
    all_todos = []
    # Scan docs (remaining)
    for doc in scan_files(DOCS_DIR, ['.md', '.txt']):
        all_todos.extend(extract_todos(doc))
    # Scan src
    for src in scan_files(SRC_DIR, ['.rs']):
        all_todos.extend(extract_todos(src))
        
    # 3. Source vs Doc (Simplified)
    implemented_fns = extract_implemented_functions(SRC_DIR)
    # In a real scenario, we'd parse docs for "Planned: function_x"
    # Here we will just list the discrepancy count as a placeholder or logic
    # if we found specific "Planned" sections. 
    # For now, we'll rely on TODOs which are explicit "Not Started" or "Partial".
    
    # 4. Generate Tasks
    tasks = []
    for i, todo in enumerate(all_todos):
        # Generate ID: TASK-<MOD>-<YYYY><SEQ>
        # Infer module from path
        path_parts = todo['file'].split(os.sep)
        module = "GEN"
        if "src" in path_parts:
            idx = path_parts.index("src")
            if idx + 1 < len(path_parts):
                module = path_parts[idx+1].upper().replace(".RS", "")
        
        task_id = f"TASK-{module}-{datetime.datetime.now().year}{i:04d}"
        
        tasks.append({
            "id": task_id,
            "description": todo['content'],
            "priority": todo['priority'],
            "file": todo['file'],
            "line": todo['line'],
            "owner": todo['owner'],
            "status": "OPEN", # Default
            "type": todo['type']
        })
        
    # 5. Deliverables
    
    # JSON
    json_output = {
        "meta": {
            "timestamp": timestamp,
            "tool": "analyze_docs.py",
            "root": PROJECT_ROOT
        },
        "summary": {
            "total_tasks": len(tasks),
            "by_priority": {
                "P0": len([t for t in tasks if t['priority'] == 'P0']),
                "P1": len([t for t in tasks if t['priority'] == 'P1']),
                "P2": len([t for t in tasks if t['priority'] == 'P2']),
                "P3": len([t for t in tasks if t['priority'] == 'P3']),
            }
        },
        "tasks": tasks
    }
    
    json_path = os.path.join(DOCS_DIR, f"unfinished_tasks_{timestamp}.json")
    with open(json_path, 'w', encoding='utf-8') as f:
        json.dump(json_output, f, indent=2, ensure_ascii=False)
        
    # Markdown
    md_path = os.path.join(DOCS_DIR, f"unfinished_tasks_summary_{timestamp}.md")
    with open(md_path, 'w', encoding='utf-8') as f:
        f.write(f"# Unfinished Tasks Report ({timestamp})\n\n")
        f.write("## Executive Summary\n")
        f.write(f"- **Total Tasks**: {len(tasks)}\n")
        f.write(f"- **Critical (P0)**: {json_output['summary']['by_priority']['P0']}\n")
        f.write(f"- **High (P1)**: {json_output['summary']['by_priority']['P1']}\n\n")
        
        f.write("## Priority Distribution\n")
        f.write("```mermaid\n")
        f.write("pie title Task Priority Distribution\n")
        for p, count in json_output['summary']['by_priority'].items():
            if count > 0:
                f.write(f'    "{p}" : {count}\n')
        f.write("```\n\n")
        
        f.write("## Top 10 High Priority Tasks\n")
        f.write("| ID | Priority | Type | Description | File |\n")
        f.write("|----|----------|------|-------------|------|\n")
        
        # Sort by priority (P0 < P1 < P2 < P3)
        sorted_tasks = sorted(tasks, key=lambda x: x['priority'])
        for t in sorted_tasks[:10]:
            desc = t['description'][:50] + "..." if len(t['description']) > 50 else t['description']
            desc = desc.replace("|", "\|")
            f.write(f"| {t['id']} | {t['priority']} | {t['type']} | {desc} | `{t['file']}:{t['line']}` |\n")
            
        f.write("\n## Risk Assessment\n")
        if json_output['summary']['by_priority']['P0'] > 0:
            f.write("⚠️ **CRITICAL RISKS DETECTED**: There are P0 tasks that may block release.\n")
        else:
            f.write("✅ No critical blockers found.\n")

    print(f"Generated reports:\n- {json_path}\n- {md_path}")

if __name__ == "__main__":
    main()
