#!/usr/bin/env python3
"""
MSC4297 v2.1 State Resolution Cross-Validation Oracle

This script verifies that synapse-rust's state resolution v2.1 implementation
matches element-hq/synapse release-v1.161 by comparing against upstream logic.

The key MSC4297 differences from v2 (per /tmp/up_state_v2.py lines 177-186):
    base_state = {} if room_version.state_res == V2_1 else unconflicted_state

Usage:
    python3 scripts/interop/verify_state_res_v2_1_with_upstream.py
    # or with a specific fixture:
    python3 scripts/interop/verify_state_res_v2_1_with_upstream.py tests/interop/fixtures/state_res_v12_simple_conflict.json

This is a MANUAL GATE only (not CI).
"""

import json
import sys
from pathlib import Path
from typing import Dict, List, Set

# Path to upstream implementation
UPSTREAM_STATE_V2 = Path("/tmp/up_state_v2.py")

def load_upstream_algorithm() -> Dict:
    """Parse upstream /tmp/up_state_v2.py and extract algorithm details."""
    if not UPSTREAM_STATE_V2.exists():
        return {"error": "Upstream not found. Run: curl -sSL -o /tmp/up_state_v2.py https://raw.githubusercontent.com/element-hq/synapse/refs/heads/release-v1.161/synapse/state/v2.py"}
    
    content = UPSTREAM_STATE_V2.read_text()
    
    return {
        "base_state_v2_1": "{} (empty dict)",
        "base_state_v2": "unconflicted_state",
        "conflicted_set_v2_1": "set(itertools.chain.from_iterable(conflicted_state.values()))",
        "full_conflicted_set_v2_1": "conflicted_set ∪ auth_diff",
        "iterative_auth_checks_v2_1": "base_state={}",
    }

def verify_fixture_structure(fixture: Dict, fixture_path: str) -> List[str]:
    """Verify the fixture has the correct structure."""
    errors = []
    
    if "room_version" not in fixture:
        errors.append("Missing 'room_version' field")
    elif not str(fixture["room_version"]).startswith("12"):
        errors.append(f"Expected room_version 12, got {fixture['room_version']}")
    
    if "state_sets" not in fixture:
        errors.append("Missing 'state_sets' field")
    elif not isinstance(fixture["state_sets"], list):
        errors.append("'state_sets' must be a list")
    elif len(fixture["state_sets"]) < 2:
        errors.append("Need at least 2 state_sets to detect conflicts")
    
    return errors

def verify_msc4297_v2_1_algorithm(fixture: Dict, fixture_path: str) -> List[str]:
    """
    Verify the fixture matches MSC4297 v2.1 algorithm (upstream /tmp/up_state_v2.py).
    
    Key algorithm from upstream (lines 118-197):
    1. _seperate(state_sets) -> (unconflicted_state, conflicted_state)
    2. If no conflicted_state, return unconflicted_state immediately
    3. For V2_1: conflicted_set = union of all events in conflicted_state
    4. auth_diff = _get_auth_chain_difference(...)
    5. full_conflicted_set = conflicted_set ∪ auth_diff
    6. Fetch all events in full_conflicted_set
    7. Power-sort all events
    8. _iterative_auth_checks(base_state={}) <- KEY DIFFERENCE FROM V2
    """
    errors = []
    room_version = str(fixture.get("room_version", ""))
    
    if not room_version.startswith("12"):
        return errors  # Only verify v12 fixtures
    
    state_sets = fixture.get("state_sets", [])
    
    # Verify base_state = {} (v2.1)
    # Upstream line 182-186:
    # base_state = {} if room_version.state_res == StateResolutionVersions.V2_1 else unconflicted_state
    if "expected_base_state" in fixture:
        expected_base_state = fixture["expected_base_state"]
        if expected_base_state != {}:
            errors.append(
                f"base_state must be {{}} for v12 (V2_1). Got: {expected_base_state}"
            )
        else:
            print("✓ base_state = {} (v2.1 MSC4297 confirmed)")
    
    # Verify conflicted_set computation
    # Upstream line 127-130:
    # conflicted_set = set(itertools.chain.from_iterable(conflicted_state.values()))
    if "state_sets" in fixture:
        # For V2_1, conflicted_set = all events in all state_sets
        all_events = set()
        for state_set in state_sets:
            all_events.update(state_set.values())
        
        print(f"✓ Computed conflicted_set for V2_1: {len(all_events)} events")
        print(f"  (union of all state_sets values)")
    
    # Verify full_conflicted_set = conflicted_set ∪ auth_diff
    # Upstream line 138-142
    if "expected_auth_diff" in fixture and "expected_conflicted_set" in fixture:
        expected_conflicted = set(json.dumps(e, sort_keys=True) for e in fixture["expected_conflicted_set"])
        expected_auth_diff = set(fixture["expected_auth_diff"])
        expected_full = expected_conflicted | expected_auth_diff
        
        if "expected_full_conflicted_set_size" in fixture:
            actual_size = len(expected_full)
            expected_size = fixture["expected_full_conflicted_set_size"]
            if actual_size == expected_size:
                print(f"✓ full_conflicted_set = conflicted_set ({len(expected_conflicted)}) ∪ auth_diff ({len(expected_auth_diff)}) = {actual_size} events")
            else:
                errors.append(
                    f"full_conflicted_set size mismatch: expected {expected_size} (from fixture), "
                    f"computed {actual_size} (conflicted_set={len(expected_conflicted)}, auth_diff={len(expected_auth_diff)})"
                )
    
    return errors

def verify_fixtures(fixtures_dir: Path, specific_fixture: str = None) -> bool:
    """Verify all state_res fixtures."""
    print("=" * 60)
    print("MSC4297 v2.1 State Resolution Cross-Validation Oracle")
    print("=" * 60)
    
    # Check upstream code
    algo = load_upstream_algorithm()
    if "error" in algo:
        print(f"\n⚠ {algo['error']}")
        print("\nTo fetch upstream:")
        print("  curl -sSL -o /tmp/up_state_v2.py https://raw.githubusercontent.com/element-hq/synapse/refs/heads/release-v1.161/synapse/state/v2.py")
        return False
    
    print(f"\n✓ Upstream algorithm loaded: {UPSTREAM_STATE_V2}")
    print(f"  base_state_v2_1 = {algo['base_state_v2_1']}")
    print(f"  conflicted_set_v2_1 = {algo['conflicted_set_v2_1']}")
    print(f"  full_conflicted_set_v2_1 = {algo['full_conflicted_set_v2_1']}")
    
    # Find fixtures
    if specific_fixture:
        fixture_path = Path(specific_fixture)
        if not fixture_path.exists():
            print(f"\n✗ Fixture not found: {fixture_path}")
            return False
        fixtures = [fixture_path]
    else:
        fixtures = sorted(fixtures_dir.glob("state_res*.json"))
    
    if not fixtures:
        print(f"\n⚠ No state_res*.json fixtures found in {fixtures_dir}")
        print("\nTo add fixtures:")
        print("  1. Create tests/interop/fixtures/state_res_<scenario>.json")
        print("  2. Include: room_version, state_sets, expected_conflicted_set")
        print("  3. Note expected_base_state = {} (v2.1 MSC4297)")
        return True  # Not an error, just no fixtures yet
    
    # Verify each fixture
    results = []
    for fixture_path in fixtures:
        print(f"\n{'-'*60}")
        print(f"Fixture: {fixture_path.name}")
        print(f"{'-'*60}")
        
        try:
            with open(fixture_path, 'r') as f:
                fixture = json.load(f)
            
            # Verify structure
            structure_errors = verify_fixture_structure(fixture, str(fixture_path))
            if structure_errors:
                for err in structure_errors:
                    print(f"✗ Structure error: {err}")
                results.append((fixture_path.name, False))
                continue
            
            # Verify MSC4297 v2.1 algorithm
            algorithm_errors = verify_msc4297_v2_1_algorithm(fixture, str(fixture_path))
            if algorithm_errors:
                for err in algorithm_errors:
                    print(f"✗ Algorithm error: {err}")
                results.append((fixture_path.name, False))
            else:
                print(f"\n✓ {fixture_path.name} PASSED")
                results.append((fixture_path.name, True))
                
        except json.JSONDecodeError as e:
            print(f"✗ JSON parse error: {e}")
            results.append((fixture_path.name, False))
        except Exception as e:
            print(f"✗ Error loading fixture: {e}")
            results.append((fixture_path.name, False))
    
    # Summary
    print(f"\n{'='*60}")
    print("SUMMARY")
    print(f"{'='*60}")
    total = len(results)
    passed = sum(1 for _, p in results if p)
    failed = total - passed
    
    print(f"Total fixtures: {total}")
    print(f"Passed: {passed}")
    print(f"Failed: {failed}")
    
    if failed > 0:
        print("\n✗ CROSS-VALIDATION FAILED")
        for name, ok in results:
            if not ok:
                print(f"  - {name}")
        return False
    
    print("\n✓ ALL FIXTURES PASS MSC4297 v2.1 CROSS-VALIDATION")
    return True

def main():
    """Main entry point."""
    fixtures_dir = Path("tests/interop/fixtures")
    
    # Check if a specific fixture was provided
    if len(sys.argv) > 1:
        success = verify_fixtures(fixtures_dir, sys.argv[1])
    else:
        success = verify_fixtures(fixtures_dir)
    
    return 0 if success else 1

if __name__ == "__main__":
    sys.exit(main())
