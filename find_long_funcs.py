import os, re
for root, _, files in os.walk("src"):
    for f in files:
        if f.endswith(".rs"):
            with open(os.path.join(root, f)) as file:
                lines = file.readlines()
                func_start = -1
                brace_count = 0
                func_name = ""
                for i, line in enumerate(lines):
                    if "fn " in line and "{" in line and func_start == -1:
                        func_start = i
                        brace_count = line.count("{") - line.count("}")
                        m = re.search(r"fn\s+(\w+)", line)
                        func_name = m.group(1) if m else "unknown"
                    elif func_start != -1:
                        brace_count += line.count("{") - line.count("}")
                        if brace_count <= 0:
                            if i - func_start > 300:
                                print(f"{os.path.join(root, f)}: {func_name} is {i - func_start} lines long")
                            func_start = -1
