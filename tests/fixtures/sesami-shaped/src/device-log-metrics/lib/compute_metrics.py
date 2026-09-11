#!/usr/bin/env python3
"""Compute per-device ES volume metrics from raw aggregation data.

Input (stdin JSON):
{
  "env_label": "preprod-emulator",
  "since": "2026-04-01T00:00:00Z",
  "until": "2026-04-01T23:59:59Z",
  "evt": { <ES aggregation response> },
  "com": { <ES aggregation response> },
  "device_counts": { <ES cardinality aggregation response> }
}

Output (stdout JSON): snapshot schema v1 (see SPEC.md)

Usage:
  cat raw.json | python3 compute_metrics.py
  python3 compute_metrics.py --recompute snapshot.json   # Phase 2: recompute with updated counts
"""

import json
import sys
from datetime import datetime, timezone


DEVICE_NAMES = {24: "Dove", 74: "Sentinel"}


def parse_time_window(since_str, until_str):
    """Parse ISO 8601 timestamps and return (since, until, hours)."""
    fmt = "%Y-%m-%dT%H:%M:%SZ"
    since = datetime.strptime(since_str, fmt).replace(tzinfo=timezone.utc)
    until = datetime.strptime(until_str, fmt).replace(tzinfo=timezone.utc)
    hours = (until - since).total_seconds() / 3600
    if hours <= 0:
        raise ValueError(f"Invalid time window: {since_str} -> {until_str} ({hours}h)")
    return since_str, until_str, hours


def parse_buckets(agg_response):
    """Extract doc_count and avg doc bytes per deviceType from ES aggregation."""
    docs = {}
    avg_bytes = {}
    buckets = (agg_response
               .get("aggregations", {})
               .get("by_device_type", {})
               .get("buckets", []))
    for b in buckets:
        dt = b["key"]
        docs[dt] = b["doc_count"]
        hits = b.get("sample", {}).get("hits", {}).get("hits", [])
        if hits:
            avg_bytes[dt] = sum(
                len(json.dumps(h["_source"])) for h in hits
            ) / len(hits)
    return docs, avg_bytes


def parse_device_counts(device_count_response):
    """Extract unique device count per deviceType from cardinality aggregation."""
    counts = {}
    if not device_count_response:
        return counts
    buckets = (device_count_response
               .get("aggregations", {})
               .get("by_device_type", {})
               .get("buckets", []))
    for b in buckets:
        counts[b["key"]] = b["unique_devices"]["value"]
    return counts


def compute(evt_data, com_data, device_count_data, since_str, until_str,
            env_label, device_count_source="cardinality"):
    """Compute snapshot from raw ES aggregation responses."""
    _, _, hours = parse_time_window(since_str, until_str)

    evt_by_type, evt_avg_bytes = parse_buckets(evt_data)
    com_by_type, com_avg_bytes = parse_buckets(com_data)
    device_counts = parse_device_counts(device_count_data)

    all_types = sorted(set(list(evt_by_type.keys()) + list(com_by_type.keys())))

    device_types = []
    for dt in all_types:
        evt_docs = evt_by_type.get(dt, 0)
        com_docs = com_by_type.get(dt, 0)
        devices = device_counts.get(dt, 0)

        if devices > 0:
            evt_per_day = evt_docs / devices / hours * 24
            com_per_day = com_docs / devices / hours * 24
            total_per_day = evt_per_day + com_per_day
            evt_sz = evt_avg_bytes.get(dt, 0)
            com_sz = com_avg_bytes.get(dt, 0)
            kb_per_day = (evt_per_day * evt_sz + com_per_day * com_sz) / 1024
        else:
            evt_per_day = 0.0
            com_per_day = 0.0
            total_per_day = 0.0
            evt_sz = evt_avg_bytes.get(dt, 0)
            com_sz = com_avg_bytes.get(dt, 0)
            kb_per_day = 0.0

        device_types.append({
            "device_type_id": dt,
            "device_type_name": DEVICE_NAMES.get(dt, str(dt)),
            "device_count": devices,
            "device_count_source": device_count_source,
            "evt_docs_per_device_per_day": round(evt_per_day, 1),
            "com_docs_per_device_per_day": round(com_per_day, 1),
            "total_docs_per_device_per_day": round(total_per_day, 1),
            "evt_avg_doc_bytes": round(evt_sz, 0),
            "com_avg_doc_bytes": round(com_sz, 0),
            "kb_per_device_per_day": round(kb_per_day, 1),
        })

    # Weighted average across device types
    total_devices = sum(d["device_count"] for d in device_types)
    if total_devices > 0:
        weighted_kb = sum(
            d["kb_per_device_per_day"] * d["device_count"]
            for d in device_types
        ) / total_devices
    else:
        weighted_kb = 0.0

    return {
        "version": 1,
        "env_label": env_label,
        "collected_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "window": {
            "since": since_str,
            "until": until_str,
            "hours": round(hours, 1),
        },
        "device_types": device_types,
        "totals": {
            "weighted_avg_kb_per_device_per_day": round(weighted_kb, 1),
        },
    }


def recompute(snapshot, new_device_counts=None):
    """Recompute metrics in an existing snapshot with updated device counts.

    Used by Phase 2 MongoDB enrichment to replace cardinality estimates
    with exact counts without re-querying ES.
    """
    if new_device_counts is None:
        return snapshot

    hours = snapshot["window"]["hours"]

    for dt_entry in snapshot["device_types"]:
        dt_id = dt_entry["device_type_id"]
        if dt_id in new_device_counts:
            old_count = dt_entry["device_count"]
            new_count = new_device_counts[dt_id]
            dt_entry["device_count"] = new_count
            dt_entry["device_count_source"] = "mongodb"

            if new_count > 0 and old_count > 0:
                # Scale per-device metrics by the count ratio
                ratio = old_count / new_count
                dt_entry["evt_docs_per_device_per_day"] = round(
                    dt_entry["evt_docs_per_device_per_day"] * ratio, 1)
                dt_entry["com_docs_per_device_per_day"] = round(
                    dt_entry["com_docs_per_device_per_day"] * ratio, 1)
                dt_entry["total_docs_per_device_per_day"] = round(
                    dt_entry["total_docs_per_device_per_day"] * ratio, 1)
                dt_entry["kb_per_device_per_day"] = round(
                    dt_entry["kb_per_device_per_day"] * ratio, 1)

    # Recompute weighted average
    total_devices = sum(d["device_count"] for d in snapshot["device_types"])
    if total_devices > 0:
        weighted_kb = sum(
            d["kb_per_device_per_day"] * d["device_count"]
            for d in snapshot["device_types"]
        ) / total_devices
    else:
        weighted_kb = 0.0

    snapshot["totals"]["weighted_avg_kb_per_device_per_day"] = round(weighted_kb, 1)
    return snapshot


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--recompute":
        # Phase 2 mode: recompute from existing snapshot with new counts on stdin
        snapshot_path = sys.argv[2]
        with open(snapshot_path) as f:
            snapshot = json.load(f)
        new_counts = json.load(sys.stdin)  # {"24": 150, "74": 30}
        new_counts = {int(k): v for k, v in new_counts.items()}
        result = recompute(snapshot, new_counts)
    else:
        # Normal mode: compute from raw ES data on stdin
        raw = json.load(sys.stdin)
        result = compute(
            evt_data=raw["evt"],
            com_data=raw["com"],
            device_count_data=raw.get("device_counts"),
            since_str=raw["since"],
            until_str=raw["until"],
            env_label=raw["env_label"],
        )

    json.dump(result, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
