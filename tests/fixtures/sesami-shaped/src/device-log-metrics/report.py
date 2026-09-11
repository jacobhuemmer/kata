#!/usr/bin/env python3
"""Render human-readable reports from snapshots or comparisons.

Usage:
  python3 report.py <snapshot.json>                    # single snapshot table
  python3 report.py --mode compare <comparison.json>   # comparison table with ratios
"""

import json
import sys

SENTINEL_IDS = {73, 74}


def _weighted_kb_avg(device_types):
    total_devices = sum(d["device_count"] for d in device_types)
    if total_devices == 0:
        return 0.0
    return sum(d["kb_per_device_per_day"] * d["device_count"] for d in device_types) / total_devices


def report_single(snapshot):
    """Render a single snapshot as a table, grouped by Dove and Sentinel."""
    w = snapshot["window"]
    label = snapshot["env_label"]

    W = 100
    header = (f"  {'Type':<6}  {'Name':<10}  {'Devices':>8}  {'Source':>6}  "
              f"{'EVT/day':>9}  {'COM/day':>9}  {'Total/day':>10}  "
              f"{'KB/day':>8}  {'MB/day':>8}")
    sep = "  " + "-" * (W - 2)

    def print_group(group_label, device_types):
        if not device_types:
            return
        print()
        print(f"  {group_label}")
        print(header)
        print(sep)
        for dt in device_types:
            dev_str = f"{dt['device_count']:,}"
            src = dt["device_count_source"][:5]
            evt = f"{dt['evt_docs_per_device_per_day']:>9,.0f}"
            com = f"{dt['com_docs_per_device_per_day']:>9,.0f}"
            total = f"{dt['total_docs_per_device_per_day']:>10,.0f}"
            kb = f"{dt['kb_per_device_per_day']:>8,.1f}"
            mb = f"{dt['kb_per_device_per_day'] / 1024:>8,.2f}"
            print(f"  {dt['device_type_id']:<6}  {dt['device_type_name']:<10}  "
                  f"{dev_str:>8}  {src:>6}  {evt}  {com}  {total}  {kb}  {mb}")
        print(sep)
        wavg = _weighted_kb_avg(device_types)
        print(f"  {'AVG':<6}  {'weighted':<10}  {'':>8}  {'':>6}  "
              f"{'':>9}  {'':>9}  {'':>10}  {wavg:>8,.1f}  {wavg / 1024:>8,.2f}")

    all_types = snapshot["device_types"]
    dove_types     = [d for d in all_types if d["device_type_id"] not in SENTINEL_IDS]
    sentinel_types = [d for d in all_types if d["device_type_id"] in SENTINEL_IDS]

    print()
    print("=" * W)
    print(f"  ES Write Volume: {label}")
    print(f"  Window: {w['since']}  ->  {w['until']}  ({w['hours']:.1f}h)")
    print("=" * W)

    print_group("Dove", dove_types)
    print_group("Sentinel", sentinel_types)

    print()
    print("=" * W)


def report_compare(comparison):
    """Render a comparison report grouped by Dove and Sentinel."""
    base   = comparison["baseline"]
    target = comparison["target"]

    W = 120
    header = (f"  {'Type':<6}  {'Name':<10}  "
              f"{'Base KB/d':>10}  {'Tgt KB/d':>10}  {'KB Ratio':>9}  "
              f"{'Base Doc/d':>10}  {'Tgt Doc/d':>10}  {'Doc Ratio':>10}  "
              f"{'Base EvtB':>9}  {'Tgt EvtB':>9}  "
              f"{'Base ComB':>9}  {'Tgt ComB':>9}")
    sep = "  " + "-" * (W - 2)

    def print_group(group_label, device_types, cat_ratio):
        if not device_types:
            return
        print()
        print(f"  {group_label}")
        print(header)
        print(sep)
        for dt in device_types:
            if dt.get("skipped"):
                print(f"  {dt['device_type_id']:<6}  {dt['device_type_name']:<10}  "
                      f"{'SKIPPED':>10}  ({dt['reason']})")
                continue
            b = dt["baseline"]
            t = dt["target"]
            kb_r  = f"{dt['kb_ratio']:>9.2f}"  if dt["kb_ratio"]   is not None else f"{'N/A':>9}"
            doc_r = f"{dt['docs_ratio']:>10.2f}" if dt["docs_ratio"] is not None else f"{'N/A':>10}"
            print(f"  {dt['device_type_id']:<6}  {dt['device_type_name']:<10}  "
                  f"{b['kb_per_device_per_day']:>10.1f}  {t['kb_per_device_per_day']:>10.1f}  {kb_r}  "
                  f"{b['docs_per_device_per_day']:>10.0f}  {t['docs_per_device_per_day']:>10.0f}  {doc_r}  "
                  f"{b['evt_avg_doc_bytes']:>9.0f}  {t['evt_avg_doc_bytes']:>9.0f}  "
                  f"{b['com_avg_doc_bytes']:>9.0f}  {t['com_avg_doc_bytes']:>9.0f}")
        print(sep)
        if cat_ratio is not None:
            base_label   = comparison["baseline"]["env_label"]
            target_label = comparison["target"]["env_label"]
            print(f"  Weighted KB ratio: {cat_ratio:.4f}  "
                  f"(1 {target_label} device = {cat_ratio:.2f} {base_label} devices)")

    all_types      = comparison["device_types"]
    dove_types     = [d for d in all_types if d["device_type_id"] not in SENTINEL_IDS]
    sentinel_types = [d for d in all_types if d["device_type_id"] in SENTINEL_IDS]

    by_cat = comparison["summary"].get("by_category", {})
    dove_ratio     = by_cat.get("dove",     {}).get("weighted_kb_ratio")
    sentinel_ratio = by_cat.get("sentinel", {}).get("weighted_kb_ratio")

    print()
    print("=" * W)
    print(f"  ES Volume Comparison")
    print(f"  Baseline: {base['env_label']}  ({base['window']['hours']:.1f}h)")
    print(f"  Target:   {target['env_label']}  ({target['window']['hours']:.1f}h)")
    print("=" * W)

    if comparison.get("warning"):
        print(f"  {comparison['warning']}")

    print_group("Dove",     dove_types,     dove_ratio)
    print_group("Sentinel", sentinel_types, sentinel_ratio)

    overall = comparison["summary"]["weighted_kb_ratio"]
    print()
    if overall is not None:
        base_label   = comparison["baseline"]["env_label"]
        target_label = comparison["target"]["env_label"]
        print(f"  Overall weighted KB ratio: {overall:.4f}")
        print(f"  1 {target_label} device = {overall:.2f} {base_label} devices in ES volume")
    print()
    print("=" * W)


def main():
    mode = "single"
    args = sys.argv[1:]

    if "--mode" in args:
        idx = args.index("--mode")
        mode = args[idx + 1]
        args = args[:idx] + args[idx + 2:]

    if not args:
        print("Usage: python3 report.py [--mode single|compare] <file.json>", file=sys.stderr)
        sys.exit(1)

    with open(args[0]) as f:
        data = json.load(f)

    # Auto-detect mode if not specified
    if mode == "single" and data.get("type") == "comparison":
        mode = "compare"

    if mode == "compare":
        report_compare(data)
    else:
        report_single(data)


if __name__ == "__main__":
    main()
