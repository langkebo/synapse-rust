#!/usr/bin/env python3
"""
Benchmark result comparison and reporting tool.

This script compares benchmark results before and after optimizations,
generating a detailed performance report.
"""

import json
import sys
from pathlib import Path
from typing import Dict, List, Any
from dataclasses import dataclass


@dataclass
class BenchmarkResult:
    """A single benchmark result."""
    name: str
    mean: float  # Average time in nanoseconds
    stddev: float
    median: float
    baseline: bool = False


@dataclass
class Comparison:
    """Comparison between baseline and optimized results."""
    operation: str
    baseline_ms: float
    optimized_ms: float
    improvement_percent: float
    speedup: float


class BenchmarkComparator:
    """Compare benchmark results and generate reports."""

    def __init__(self, baseline_path: str, optimized_path: str):
        self.baseline_path = Path(baseline_path)
        self.optimized_path = Path(optimized_path)
        self.baseline_results = self.load_results(self.baseline_path)
        self.optimized_results = self.load_results(self.optimized_path)

    def load_results(self, path: Path) -> Dict[str, BenchmarkResult]:
        """Load Criterion benchmark results from JSON file."""
        results = {}

        with open(path) as f:
            data = json.load(f)

        for group in data.get("groups", []):
            group_name = group.get("group_name", "")
            for bench in group.get("benches", []):
                name = f"{group_name}/{bench.get('name', '')}"
                # Convert from nanoseconds to milliseconds
                mean_ns = bench.get("mean", {}).get("estimate", 0)
                mean_ms = mean_ns / 1_000_000

                results[name] = BenchmarkResult(
                    name=name,
                    mean=mean_ms,
                    stddev=bench.get("stddev", {}).get("estimate", 0) / 1_000_000,
                    median=bench.get("median", {}).get("estimate", 0) / 1_000_000,
                )

        return results

    def compare(self) -> List[Comparison]:
        """Generate comparisons between baseline and optimized results."""
        comparisons = []

        for name, baseline in self.baseline_results.items():
            if name in self.optimized_results:
                optimized = self.optimized_results[name]

                improvement = (
                    (baseline.mean - optimized.mean) / baseline.mean * 100
                    if baseline.mean > 0
                    else 0
                )

                speedup = (
                    baseline.mean / optimized.mean
                    if optimized.mean > 0
                    else 1.0
                )

                comparisons.append(Comparison(
                    operation=name,
                    baseline_ms=baseline.mean,
                    optimized_ms=optimized.mean,
                    improvement_percent=improvement,
                    speedup=speedup,
                ))

        return sorted(comparisons, key=lambda c: c.improvement_percent, reverse=True)

    def generate_report(self) -> str:
        """Generate a markdown performance report."""
        comparisons = self.compare()

        report = [
            "# Benchmark Performance Report\n",
            f"**Baseline**: `{self.baseline_path.name}`\n",
            f"**Optimized**: `{self.optimized_path.name}`\n",
            f"**Date**: {self.get_current_date()}\n",
            "\n---\n",
            "## Summary\n",
            "\n",
            f"Total benchmarks: {len(comparisons)}\n",
            "\n",
        ]

        # Calculate overall statistics
        improvements = [c.improvement_percent for c in comparisons]
        avg_improvement = sum(improvements) / len(improvements) if improvements else 0

        excellent = sum(1 for i in improvements if i >= 80)
        good = sum(1 for i in improvements if 50 <= i < 80)
        moderate = sum(1 for i in improvements if 20 <= i < 50)
        minimal = sum(1 for i in improvements if 0 <= i < 20)
        regression = sum(1 for i in improvements if i < 0)

        report.extend([
            f"- **Average Improvement**: {avg_improvement:.1f}%\n",
            f"- **Excellent (≥80%)**: {excellent}\n",
            f"- **Good (≥50%)**: {good}\n",
            f"- **Moderate (≥20%)**: {moderate}\n",
            f"- **Minimal (>0%)**: {minimal}\n",
            f"- **Regression (<0%)**: {regression}\n",
            "\n---\n",
            "## Detailed Results\n",
            "\n",
            "| Operation | Baseline (ms) | Optimized (ms) | Improvement | Speedup |\n",
            "|-----------|---------------|----------------|-------------|---------|\n",
        ])

        for c in comparisons:
            status = self.get_status_icon(c.improvement_percent)
            report.append(
                f"| {status} {c.operation} | {c.baseline_ms:.3f} | {c.optimized_ms:.3f} | "
                f"{c.improvement_percent:+.1f}% | {c.speedup:.2f}x |\n"
            )

        report.extend([
            "\n---\n",
            "## Performance Categories\n",
            "\n",
            "### 🚀 Excellent Improvements (≥80%)\n",
            "\n",
        ])

        excellent_ops = [c for c in comparisons if c.improvement_percent >= 80]
        if excellent_ops:
            for c in excellent_ops:
                report.append(f"- **{c.operation}**: {c.improvement_percent:.1f}% faster ({c.speedup:.2f}x)\n")
        else:
            report.append("None\n")

        report.extend([
            "\n",
            "### ✅ Good Improvements (≥50%)\n",
            "\n",
        ])

        good_ops = [c for c in comparisons if 50 <= c.improvement_percent < 80]
        if good_ops:
            for c in good_ops:
                report.append(f"- **{c.operation}**: {c.improvement_percent:.1f}% faster ({c.speedup:.2f}x)\n")
        else:
            report.append("None\n")

        report.extend([
            "\n",
            "### ⚠️ Moderate Improvements (≥20%)\n",
            "\n",
        ])

        moderate_ops = [c for c in comparisons if 20 <= c.improvement_percent < 50]
        if moderate_ops:
            for c in moderate_ops:
                report.append(f"- **{c.operation}**: {c.improvement_percent:.1f}% faster\n")
        else:
            report.append("None\n")

        report.extend([
            "\n",
            "## Recommendations\n",
            "\n",
        ])

        if regression > 0:
            report.append(f"⚠️ **Warning**: {regression} benchmarks show degraded performance. Review these operations.\n\n")

        if avg_improvement >= 50:
            report.append("✅ **Excellent**: Overall performance improvement meets targets!\n\n")
        elif avg_improvement >= 20:
            report.append("✅ **Good**: Performance improvement is significant.\n\n")
        else:
            report.append("📝 **Note**: Consider further optimization opportunities.\n\n")

        return "".join(report)

    def get_status_icon(self, improvement: float) -> str:
        """Get status emoji based on improvement percentage."""
        if improvement >= 80:
            return "🚀"
        elif improvement >= 50:
            return "✅"
        elif improvement >= 20:
            return "⚠️"
        elif improvement > 0:
            return "📝"
        else:
            return "❌"

    @staticmethod
    def get_current_date() -> str:
        """Get current date in ISO format."""
        from datetime import datetime
        return datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    def save_report(self, output_path: str = "BENCHMARK_REPORT.md"):
        """Save the report to a file."""
        report = self.generate_report()

        with open(output_path, "w") as f:
            f.write(report)

        print(f"Report saved to: {output_path}")
        print(f"\n{report}")


def main():
    """Main entry point."""
    if len(sys.argv) < 3:
        print("Usage: python compare_results.py <baseline.json> <optimized.json> [output.md]")
        print("\nExample:")
        print("  python compare_results.py baseline/bench.json optimized/bench.json BENCHMARK_REPORT.md")
        sys.exit(1)

    baseline_path = sys.argv[1]
    optimized_path = sys.argv[2]
    output_path = sys.argv[3] if len(sys.argv) > 3 else "BENCHMARK_REPORT.md"

    comparator = BenchmarkComparator(baseline_path, optimized_path)
    comparator.save_report(output_path)


if __name__ == "__main__":
    main()
