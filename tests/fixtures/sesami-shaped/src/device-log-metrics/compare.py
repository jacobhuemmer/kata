#!/usr/bin/env python3
"""Compare two ES volume snapshots and compute equivalence ratios.

Usage:
  python3 compare.py <snapshot_a.json> <snapshot_b.json>

The first snapshot is treated as the "baseline" (e.g., emulator),
the second as the "target" (e.g., real devices).

Output:
  - Comparison JSON to snapshots/comparison_{timestamp}.json
  - Summary line to stdout
"""

import json
import sys
import os
from datetime import datetime, timezone

SENTINEL_IDS = {73, 74}


def load_snapshot(path):
    """Load and validate a snapshot JSON file."""
    with open(path) as f:
        data = json.load(f)
    if data.get("version") != 1:
        raise ValueError(f"{path}: expected schema version 1, got {data.get('version')}")
    return data


def check_window_mismatch(a, b):
    """Warn if time windows differ by more than 20%."""
    hours_a = a["window"]["hours"]
    hours_b = b["window"]["hours"]
    if hours_a == 0 or hours_b == 0:
        return "WARNING: one or both snapshots have a zero-hour window"
    ratio = max(hours_a, hours_b) / min(hours_a, hours_b)
    if ratio > 1.2:
        return (f"WARNING: time windows differ by {(ratio - 1) * 100:.0f}% "
                f"({hours_a:.1f}h vs {hours_b:.1f}h). Results may not be comparable.")
    return None


def build_type_map(snapshot):
    """Index device_types by device_type_id."""
    return {dt["device_type_id"]: dt for dt in snapshot["device_types"]}


def compare(baseline, target):
    """Compare two snapshots and return comparison data."""
    warning = check_window_mismatch(baseline, target)

    base_map = build_type_map(baseline)
    target_map = build_type_map(target)

    all_types = sorted(set(list(base_map.keys()) + list(target_map.keys())))

    comparisons = []
    for dt_id in all_types:
        base_dt = base_map.get(dt_id)
        target_dt = target_map.get(dt_id)

        if base_dt is None or target_dt is None:
            comparisons.append({
                "device_type_id": dt_id,
                "device_type_name": (base_dt or target_dt)["device_type_name"],
                "skipped": True,
                "reason": f"missing from {'baseline' if base_dt is None else 'target'}",
            })
            continue

        base_kb = base_dt["kb_per_device_per_day"]
        target_kb = target_dt["kb_per_device_per_day"]

        if base_kb > 0:
            kb_ratio = target_kb / base_kb
        else:
            kb_ratio = None

        base_docs = base_dt["total_docs_per_device_per_day"]
        target_docs = target_dt["total_docs_per_device_per_day"]

        if base_docs > 0:
            docs_ratio = target_docs / base_docs
        else:
            docs_ratio = None

        comparisons.append({
            "device_type_id": dt_id,
            "device_type_name": base_dt["device_type_name"],
            "skipped": False,
            "baseline": {
                "env_label": baseline["env_label"],
                "device_count": base_dt["device_count"],
                "device_count_source": base_dt["device_count_source"],
                "docs_per_device_per_day": base_docs,
                "kb_per_device_per_day": base_kb,
                "evt_avg_doc_bytes": base_dt["evt_avg_doc_bytes"],
                "com_avg_doc_bytes": base_dt["com_avg_doc_bytes"],
            },
            "target": {
                "env_label": target["env_label"],
                "device_count": target_dt["device_count"],
                "device_count_source": target_dt["device_count_source"],
                "docs_per_device_per_day": target_docs,
                "kb_per_device_per_day": target_kb,
                "evt_avg_doc_bytes": target_dt["evt_avg_doc_bytes"],
                "com_avg_doc_bytes": target_dt["com_avg_doc_bytes"],
            },
            "kb_ratio": round(kb_ratio, 4) if kb_ratio is not None else None,
            "docs_ratio": round(docs_ratio, 4) if docs_ratio is not None else None,
        })

    def _weighted_ratio(entries):
        total = sum(c["baseline"]["device_count"] for c in entries)
        if total == 0:
            return None
        return sum(c["kb_ratio"] * c["baseline"]["device_count"] for c in entries) / total

    valid = [c for c in comparisons if not c["skipped"] and c["kb_ratio"] is not None]
    dove_valid     = [c for c in valid if c["device_type_id"] not in SENTINEL_IDS]
    sentinel_valid = [c for c in valid if c["device_type_id"] in SENTINEL_IDS]

    overall_ratio  = _weighted_ratio(valid)
    dove_ratio     = _weighted_ratio(dove_valid)
    sentinel_ratio = _weighted_ratio(sentinel_valid)

    result = {
        "version": 1,
        "type": "comparison",
        "compared_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "baseline": {
            "env_label": baseline["env_label"],
            "window": baseline["window"],
        },
        "target": {
            "env_label": target["env_label"],
            "window": target["window"],
        },
        "warning": warning,
        "device_types": comparisons,
        "summary": {
            "weighted_kb_ratio": round(overall_ratio, 4) if overall_ratio is not None else None,
            "by_category": {
                "dove":     {"weighted_kb_ratio": round(dove_ratio, 4) if dove_ratio is not None else None},
                "sentinel": {"weighted_kb_ratio": round(sentinel_ratio, 4) if sentinel_ratio is not None else None},
            },
        },
    }

    return result


def main():
    if len(sys.argv) != 3:
        print("Usage: python3 compare.py <baseline.json> <target.json>", file=sys.stderr)
        sys.exit(1)

    baseline = load_snapshot(sys.argv[1])
    target = load_snapshot(sys.argv[2])

    result = compare(baseline, target)

    # Write comparison file
    script_dir = os.path.dirname(os.path.abspath(__file__))
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    out_path = os.path.join(script_dir, "snapshots", f"comparison_{timestamp}.json")
    with open(out_path, "w") as f:
        json.dump(result, f, indent=2)
        f.write("\n")

    print(f"Comparison written to: {out_path}", file=sys.stderr)

    # Print warning if any
    if result["warning"]:
        print(result["warning"], file=sys.stderr)

    # Print per-category summary
    base_label   = result["baseline"]["env_label"]
    target_label = result["target"]["env_label"]
    by_cat = result["summary"]["by_category"]

    for cat_name, cat in [("Dove", by_cat["dove"]), ("Sentinel", by_cat["sentinel"])]:
        ratio = cat["weighted_kb_ratio"]
        if ratio is not None:
            print(f"[{cat_name}]  1 {target_label} device = {ratio:.2f} {base_label} devices in ES volume")
        else:
            print(f"[{cat_name}]  Could not compute ratio (insufficient data)")

    overall = result["summary"]["weighted_kb_ratio"]
    if overall is not None:
        print(f"[Overall] 1 {target_label} device = {overall:.2f} {base_label} devices in ES volume")


if __name__ == "__main__":
    main()
