#!/usr/bin/env python3
"""
Guardrail 性能测试评估器

用途：
- 解析 k6 测试结果（JSON 格式）
- 验证性能阈值
- 生成彩色报告

用法:
    python guardrail.py results.json
    python guardrail.py results.json --verbose
"""

import json
import sys
from pathlib import Path


class Colors:
    """ANSI 颜色代码"""
    RESET = '\033[0m'
    RED = '\033[91m'
    GREEN = '\033[92m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    BOLD = '\033[1m'

def print_colored(text, color):
    """打印带颜色的文本"""
    print(f"{color}{text}{Colors.RESET}")

def print_header(title):
    """打印标题"""
    print_colored("\n" + "=" * 50, Colors.BLUE)
    print_colored(f" {title}", Colors.BOLD)
    print_colored("=" * 50 + "\n", Colors.BLUE)

def check_threshold(name, actual, threshold, operator='<'):
    """
    检查单个阈值
    
    Args:
        name: 指标名称
        actual: 实际值
        threshold: 阈值
        operator: '<' 或 '>'
    
    Returns:
        (bool, str) 是否通过，状态图标
    """
    try:
        if operator == '<':
            passed = actual < threshold
        else:
            passed = actual > threshold
        
        if passed:
            icon = "✓ PASS"
            color = Colors.GREEN
        else:
            icon = "✗ FAIL"
            color = Colors.RED
        
        print_colored(f"  {icon}: {name} = {actual:.2f} ({operator} {threshold})", color)
        return passed, icon
    except (TypeError, ValueError):
        print_colored(f"  ⚠ SKIP: {name} = {actual} (无法比较)", Colors.YELLOW)
        return None, "⚠ SKIP"

def analyze_flat_results(json_path):
    """分析扁平结构的 k6 结果（标准格式）"""
    with open(json_path) as f:
        data = json.load(f)
    
    return extract_metrics(data)

def analyze_nested_results(json_path):
    """分析嵌套结构的 k6 结果"""
    with open(json_path) as f:
        data = json.load(f)
    
    # 如果是嵌套结构，提取 metrics 部分
    if 'data' in data and 'metrics' in data['data']:
        return extract_metrics(data['data'])
    elif 'metrics' in data:
        return extract_metrics(data)
    else:
        raise ValueError("Unknown result format")

def extract_metrics(data):
    """从数据中提取关键指标"""
    metrics = {}
    
    if 'metrics' in data:
        metrics_data = data['metrics']
        
        # HTTP 请求次数
        if 'http_reqs' in metrics_data:
            metrics['http_reqs'] = metrics_data['http_reqs'].get('values', {}).get('count', 0)
        
        # 错误率
        if 'http_req_failed' in metrics_data:
            metrics['error_rate'] = metrics_data['http_req_failed'].get('values', {}).get('rate', 0) * 100
        
        # 响应时间分位数
        if 'http_req_duration' in metrics_data:
            duration_values = metrics_data['http_req_duration'].get('values', {})
            metrics['p50'] = duration_values.get('p(50)', 0)
            metrics['p90'] = duration_values.get('p(90)', 0)
            metrics['p95'] = duration_values.get('p(95)', 0)
            metrics['p99'] = duration_values.get('p(99)', 0)
            metrics['avg'] = duration_values.get('avg', 0)
            metrics['max'] = duration_values.get('max', 0)
        
        # 自定义指标
        if 'errors' in metrics_data:
            metrics['error_count'] = metrics_data['errors'].get('values', {}).get('count', 0)
    
    # 测试持续时间
    if 'state' in data:
        metrics['duration_sec'] = data['state'].get('testRunDurationMs', 0) / 1000
    
    return metrics

def generate_report(json_path, verbose=False):
    """生成完整评估报告"""
    print_header(f"性能测试评估：{Path(json_path).name}")
    
    # 检测文件格式并分析
    try:
        metrics = analyze_flat_results(json_path)
    except Exception:
        try:
            metrics = analyze_nested_results(json_path)
        except Exception as e:
            print_colored(f"无法解析文件：{e}", Colors.RED)
            sys.exit(1)
    
    # 输出基础信息
    print(f"\n基础信息:")
    print(f"  请求总数:  {metrics.get('http_reqs', 0):,.0f}")
    print(f"  测试时长:  {metrics.get('duration_sec', 0):.1f}s")
    print(f"  平均 TPS:  {(metrics.get('http_reqs', 0) / max(metrics.get('duration_sec', 1), 0.1)):.1f}")
    
    # 错误率
    error_rate = metrics.get('error_rate', 0)
    print(f"\n错误率:")
    if error_rate < 1:
        print_colored(f"  ✓ 错误率：{error_rate:.2f}% (< 1%)", Colors.GREEN)
    elif error_rate < 5:
        print_colored(f"  ⚠ 错误率：{error_rate:.2f}% (1-5%)", Colors.YELLOW)
    else:
        print_colored(f"  ✗ 错误率：{error_rate:.2f}% (> 5%)", Colors.RED)
    
    # 响应时间分位数
    print_header("响应时间 (ms)")
    print_colored("  平均值:", Colors.BLUE, end=" ")
    print(f"{metrics.get('avg', 0):.1f}ms")
    
    print_colored("  P50:", Colors.BLUE, end=" ")
    print(f"{metrics.get('p50', 0):.1f}ms")
    
    print_colored("  P90:", Colors.BLUE, end=" ")
    print(f"{metrics.get('p90', 0):.1f}ms")
    
    print_colored("  P95:", Colors.BLUE, end=" ")
    p95_val = metrics.get('p95', 0)
    if p95_val < 200:
        print_colored(f"{p95_val:.1f}ms (< 200ms)", Colors.GREEN)
    elif p95_val < 500:
        print_colored(f"{p95_val:.1f}ms (200-500ms)", Colors.YELLOW)
    else:
        print_colored(f"{p95_val:.1f}ms (> 500ms)", Colors.RED)
    
    print_colored("  P99:", Colors.BLUE, end=" ")
    p99_val = metrics.get('p99', 0)
    if p99_val < 500:
        print_colored(f"{p99_val:.1f}ms (< 500ms)", Colors.GREEN)
    elif p99_val < 1000:
        print_colored(f"{p99_val:.1f}ms (500ms-1s)", Colors.YELLOW)
    else:
        print_colored(f"{p99_val:.1f}ms (> 1s)", Colors.RED)
    
    print_colored("  最大值:", Colors.BLUE, end=" ")
    print(f"{metrics.get('max', 0):.1f}ms")
    
    # 详细阈值检查
    if verbose:
        print_header("详细阈值检查")
        thresholds = [
            ('P50', metrics.get('p50', 0), 100, '<'),
            ('P90', metrics.get('p90', 0), 200, '<'),
            ('P95', metrics.get('p95', 0), 500, '<'),
            ('P99', metrics.get('p99', 0), 1000, '<'),
            ('Error Rate', metrics.get('error_rate', 0), 1.0, '<'),
        ]
        
        results = []
        for name, actual, threshold, op in thresholds:
            passed, icon = check_threshold(name, actual, threshold, op)
            results.append(passed)
        
        # 总结
        passed_count = sum(1 for r in results if r is True)
        total = len([r for r in results if r is not None])
        
        print_header("总结")
        print(f"  通过的阈值：{passed_count}/{total}")
        
        if all(r is True for r in results if r is not None):
            print_colored("\n  ✓ 所有阈值通过!", Colors.GREEN)
            sys.exit(0)
        else:
            print_colored("\n  ✗ 部分阈值未通过", Colors.RED)
            sys.exit(1)
    else:
        print_header("简略模式")
        print("  使用 --verbose 查看详细阈值检查结果")
        print(f"\n  结果文件：{json_path}")
        sys.exit(0)


def main():
    if len(sys.argv) < 2:
        print(f"用法：python {sys.argv[0]} <result.json> [--verbose]")
        print(f"\n示例:")
        print(f"  python {sys.argv[0]} results/smoke_20260930_120000.json")
        print(f"  python {sys.argv[0]} results/light_20260930_120000.json --verbose")
        sys.exit(1)
    
    json_path = sys.argv[1]
    verbose = '--verbose' in sys.argv
    
    if not Path(json_path).exists():
        print(f"错误：文件不存在：{json_path}")
        sys.exit(1)
    
    generate_report(json_path, verbose)


if __name__ == '__main__':
    main()
