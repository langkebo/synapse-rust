# Format Drift Tracking

This report keeps the most recent three delivery-cycle checks after the repository-wide formatting rollout.
Update it after each release train, sprint handoff, or other agreed delivery checkpoint.

## Workflow

1. Run `make format-check` or wait for the scheduled `Format Drift Tracking` workflow.
2. Run `make format-cycle CYCLE_LABEL=<cycle-name>` to refresh this report locally.
3. Review any new conflicts, nested formatter configs, or drift signals before the next cycle starts.

## Latest Snapshot

- Latest cycle: `cycle-1-2026-05-29`
- Range: `HEAD~1..HEAD`
- Head commit: `dc14b6c`
- Compliance status: `pass`
- Drift signals: `0`
- Newly introduced nested formatter configs: `0`

## Cycle Log

| Cycle | Range | Commits | Changed Format Files | Drift Signals | New Nested Configs | Compliance | Status |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| `cycle-1-2026-05-29` | `HEAD~1..HEAD` | 1 | 629 | 0 | 0 | `pass` | `PASS` |

## Current Gates

- `PASS` means the cycle check ran after a successful compliance run, found zero drift signals, and did not introduce new nested formatter configs.
- `ATTN` means at least one of those conditions failed and maintainers should inspect the cycle details before closing the checkpoint.

## Latest Cycle Details

- Cycle label: `cycle-1-2026-05-29`
- Generated at: `2026-05-29 13:08:05Z`
- Changed files in range: `634`
- Changed format-scoped files in range: `629`
- Drift totals: `trailing_ws=0`, `crlf=0`, `tabs=0`, `missing_final_newline=0`
- Tracked nested formatter configs: `0`

## Open Conflicts

- VS Code Rust rulers [100] do not match rustfmt max_width=120.

<!-- format-drift-tracking-state:start -->
{
  "history": [
    {
      "cycle_label": "cycle-1-2026-05-29",
      "generated_at": "2026-05-29 13:08:05Z",
      "base_ref": "HEAD~1",
      "head_ref": "HEAD",
      "head_commit": "dc14b6c",
      "commit_count": 1,
      "changed_file_count": 634,
      "changed_format_file_count": 629,
      "compliance_status": "pass",
      "drift_signal_total": 0,
      "drift_totals": {
        "trailing_ws": 0,
        "crlf": 0,
        "tabs": 0,
        "missing_final_newline": 0
      },
      "nested_config_count": 0,
      "nested_configs": [],
      "new_nested_configs": [],
      "conflicts": [
        "VS Code Rust rulers [100] do not match rustfmt max_width=120."
      ],
      "status": "pass"
    }
  ]
}
<!-- format-drift-tracking-state:end -->
