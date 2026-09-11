#!/usr/bin/env python3
"""Tests for lib/compute_metrics.py"""

import json
import os
import sys
import unittest

# Add parent dir so we can import the library
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))
import compute_metrics as cm


def make_es_agg(buckets):
    """Build a minimal ES aggregation response from bucket defs.

    Each bucket: {"key": deviceTypeId, "doc_count": N, "docs": [{"field": "val"}, ...]}
    """
    es_buckets = []
    for b in buckets:
        hits = [{"_source": doc} for doc in b.get("docs", [{"x": "y"}] * 5)]
        es_buckets.append({
            "key": b["key"],
            "doc_count": b["doc_count"],
            "sample": {"hits": {"hits": hits}},
        })
    return {"aggregations": {"by_device_type": {"buckets": es_buckets}}}


def make_device_count_agg(counts):
    """Build a cardinality aggregation response. counts: {deviceTypeId: count}"""
    buckets = [{"key": k, "unique_devices": {"value": v}} for k, v in counts.items()]
    return {"aggregations": {"by_device_type": {"buckets": buckets}}}


class TestParseTimeWindow(unittest.TestCase):
    def test_valid_window(self):
        since, until, hours = cm.parse_time_window(
            "2026-04-01T00:00:00Z", "2026-04-02T00:00:00Z")
        self.assertEqual(hours, 24.0)

    def test_12h_window(self):
        _, _, hours = cm.parse_time_window(
            "2026-04-01T00:00:00Z", "2026-04-01T12:00:00Z")
        self.assertEqual(hours, 12.0)

    def test_invalid_window(self):
        with self.assertRaises(ValueError):
            cm.parse_time_window("2026-04-02T00:00:00Z", "2026-04-01T00:00:00Z")


class TestParseBuckets(unittest.TestCase):
    def test_basic(self):
        agg = make_es_agg([
            {"key": 24, "doc_count": 1000, "docs": [{"a": "b"}, {"c": "d"}]},
        ])
        docs, avg_bytes = cm.parse_buckets(agg)
        self.assertEqual(docs[24], 1000)
        self.assertIn(24, avg_bytes)
        self.assertGreater(avg_bytes[24], 0)

    def test_empty(self):
        agg = {"aggregations": {"by_device_type": {"buckets": []}}}
        docs, avg_bytes = cm.parse_buckets(agg)
        self.assertEqual(docs, {})
        self.assertEqual(avg_bytes, {})


class TestCompute(unittest.TestCase):
    def _run_compute(self, evt_buckets, com_buckets, device_counts):
        evt = make_es_agg(evt_buckets)
        com = make_es_agg(com_buckets)
        dc = make_device_count_agg(device_counts)
        return cm.compute(evt, com, dc,
                          "2026-04-01T00:00:00Z", "2026-04-02T00:00:00Z",
                          "test-env")

    def test_single_device_type(self):
        # 100 devices, 2400 evt docs in 24h = 1 evt/device/day
        result = self._run_compute(
            evt_buckets=[{"key": 24, "doc_count": 2400}],
            com_buckets=[{"key": 24, "doc_count": 1200}],
            device_counts={24: 100},
        )
        self.assertEqual(result["version"], 1)
        self.assertEqual(len(result["device_types"]), 1)
        dt = result["device_types"][0]
        self.assertEqual(dt["device_type_id"], 24)
        self.assertEqual(dt["device_count"], 100)
        self.assertEqual(dt["device_count_source"], "cardinality")
        self.assertAlmostEqual(dt["evt_docs_per_device_per_day"], 24.0, places=0)
        self.assertAlmostEqual(dt["com_docs_per_device_per_day"], 12.0, places=0)
        self.assertAlmostEqual(dt["total_docs_per_device_per_day"], 36.0, places=0)
        self.assertGreater(dt["kb_per_device_per_day"], 0)

    def test_multiple_device_types(self):
        result = self._run_compute(
            evt_buckets=[
                {"key": 24, "doc_count": 2400},
                {"key": 74, "doc_count": 480},
            ],
            com_buckets=[
                {"key": 24, "doc_count": 1200},
                {"key": 74, "doc_count": 240},
            ],
            device_counts={24: 100, 74: 20},
        )
        self.assertEqual(len(result["device_types"]), 2)
        self.assertGreater(result["totals"]["weighted_avg_kb_per_device_per_day"], 0)

    def test_zero_devices(self):
        result = self._run_compute(
            evt_buckets=[{"key": 24, "doc_count": 100}],
            com_buckets=[],
            device_counts={},  # no devices found
        )
        dt = result["device_types"][0]
        self.assertEqual(dt["device_count"], 0)
        self.assertEqual(dt["evt_docs_per_device_per_day"], 0.0)
        self.assertEqual(dt["kb_per_device_per_day"], 0.0)

    def test_missing_com_stream(self):
        """Device type appears in evt but not com."""
        result = self._run_compute(
            evt_buckets=[{"key": 24, "doc_count": 2400}],
            com_buckets=[],  # no com data
            device_counts={24: 100},
        )
        dt = result["device_types"][0]
        self.assertEqual(dt["com_docs_per_device_per_day"], 0.0)
        self.assertGreater(dt["evt_docs_per_device_per_day"], 0)

    def test_weighted_average(self):
        """Weighted avg should lean toward the type with more devices."""
        result = self._run_compute(
            evt_buckets=[
                {"key": 24, "doc_count": 10000},  # high volume
                {"key": 74, "doc_count": 100},     # low volume
            ],
            com_buckets=[
                {"key": 24, "doc_count": 5000},
                {"key": 74, "doc_count": 50},
            ],
            device_counts={24: 1000, 74: 10},  # 24 dominates
        )
        dt_24 = next(d for d in result["device_types"] if d["device_type_id"] == 24)
        weighted = result["totals"]["weighted_avg_kb_per_device_per_day"]
        # Weighted avg should be close to dt_24's value since it has 99% of devices
        self.assertAlmostEqual(weighted, dt_24["kb_per_device_per_day"], delta=1.0)


class TestRecompute(unittest.TestCase):
    def test_recompute_scales_metrics(self):
        """Recompute with doubled device count should halve per-device metrics."""
        snapshot = {
            "version": 1,
            "env_label": "test",
            "collected_at": "2026-04-01T12:00:00Z",
            "window": {"since": "...", "until": "...", "hours": 24.0},
            "device_types": [{
                "device_type_id": 24,
                "device_type_name": "Dove",
                "device_count": 100,
                "device_count_source": "cardinality",
                "evt_docs_per_device_per_day": 24.0,
                "com_docs_per_device_per_day": 12.0,
                "total_docs_per_device_per_day": 36.0,
                "evt_avg_doc_bytes": 280,
                "com_avg_doc_bytes": 350,
                "kb_per_device_per_day": 10.0,
            }],
            "totals": {"weighted_avg_kb_per_device_per_day": 10.0},
        }
        result = cm.recompute(snapshot, {24: 200})
        dt = result["device_types"][0]
        self.assertEqual(dt["device_count"], 200)
        self.assertEqual(dt["device_count_source"], "mongodb")
        self.assertAlmostEqual(dt["evt_docs_per_device_per_day"], 12.0, places=1)
        self.assertAlmostEqual(dt["kb_per_device_per_day"], 5.0, places=1)

    def test_recompute_no_counts(self):
        """Recompute with None counts returns snapshot unchanged."""
        snapshot = {"device_types": [], "totals": {"weighted_avg_kb_per_device_per_day": 0}}
        result = cm.recompute(snapshot, None)
        self.assertEqual(result, snapshot)


if __name__ == "__main__":
    unittest.main()
