#!/usr/bin/env python3
"""Tests for compare.py"""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import compare as cmp


FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")


def load_fixture(name):
    with open(os.path.join(FIXTURES, name)) as f:
        return json.load(f)


def make_snapshot(env_label, device_types, hours=24.0):
    """Build a minimal snapshot for testing."""
    return {
        "version": 1,
        "env_label": env_label,
        "collected_at": "2026-04-01T12:00:00Z",
        "window": {
            "since": "2026-03-31T12:00:00Z",
            "until": "2026-04-01T12:00:00Z",
            "hours": hours,
        },
        "device_types": device_types,
        "totals": {"weighted_avg_kb_per_device_per_day": 0},
    }


def make_device_type(dt_id=24, name="Dove", count=100, kb=50.0, docs=100.0,
                     evt_bytes=280, com_bytes=350):
    return {
        "device_type_id": dt_id,
        "device_type_name": name,
        "device_count": count,
        "device_count_source": "cardinality",
        "evt_docs_per_device_per_day": docs * 0.7,
        "com_docs_per_device_per_day": docs * 0.3,
        "total_docs_per_device_per_day": docs,
        "evt_avg_doc_bytes": evt_bytes,
        "com_avg_doc_bytes": com_bytes,
        "kb_per_device_per_day": kb,
    }


class TestWindowMismatch(unittest.TestCase):
    def test_no_warning_same_window(self):
        a = make_snapshot("a", [], hours=24.0)
        b = make_snapshot("b", [], hours=24.0)
        self.assertIsNone(cmp.check_window_mismatch(a, b))

    def test_no_warning_within_threshold(self):
        a = make_snapshot("a", [], hours=24.0)
        b = make_snapshot("b", [], hours=27.0)  # 12.5% diff, under 20%
        self.assertIsNone(cmp.check_window_mismatch(a, b))

    def test_warning_over_threshold(self):
        a = make_snapshot("a", [], hours=24.0)
        b = make_snapshot("b", [], hours=30.0)  # 25% diff
        warning = cmp.check_window_mismatch(a, b)
        self.assertIn("WARNING", warning)
        self.assertIn("25%", warning)

    def test_warning_zero_window(self):
        a = make_snapshot("a", [], hours=0)
        b = make_snapshot("b", [], hours=24.0)
        warning = cmp.check_window_mismatch(a, b)
        self.assertIn("zero", warning.lower())


class TestCompare(unittest.TestCase):
    def test_ratio_computation(self):
        baseline = make_snapshot("emulator", [make_device_type(kb=50.0, docs=100.0)])
        target = make_snapshot("real", [make_device_type(kb=100.0, docs=200.0)])
        result = cmp.compare(baseline, target)

        self.assertEqual(len(result["device_types"]), 1)
        dt = result["device_types"][0]
        self.assertFalse(dt["skipped"])
        self.assertAlmostEqual(dt["kb_ratio"], 2.0, places=2)
        self.assertAlmostEqual(dt["docs_ratio"], 2.0, places=2)

    def test_identical_snapshots(self):
        snap = make_snapshot("test", [make_device_type(kb=50.0, docs=100.0)])
        result = cmp.compare(snap, snap)

        dt = result["device_types"][0]
        self.assertAlmostEqual(dt["kb_ratio"], 1.0, places=4)
        self.assertAlmostEqual(dt["docs_ratio"], 1.0, places=4)
        self.assertAlmostEqual(result["summary"]["weighted_kb_ratio"], 1.0, places=4)

    def test_missing_device_type_in_target(self):
        baseline = make_snapshot("emu", [
            make_device_type(dt_id=24, kb=50.0),
            make_device_type(dt_id=74, name="Sentinel", kb=30.0),
        ])
        target = make_snapshot("real", [
            make_device_type(dt_id=24, kb=100.0),
            # dt_id=74 missing
        ])
        result = cmp.compare(baseline, target)

        types_by_id = {d["device_type_id"]: d for d in result["device_types"]}
        self.assertFalse(types_by_id[24]["skipped"])
        self.assertTrue(types_by_id[74]["skipped"])
        self.assertIn("target", types_by_id[74]["reason"])

    def test_missing_device_type_in_baseline(self):
        baseline = make_snapshot("emu", [make_device_type(dt_id=24, kb=50.0)])
        target = make_snapshot("real", [
            make_device_type(dt_id=24, kb=100.0),
            make_device_type(dt_id=74, name="Sentinel", kb=30.0),
        ])
        result = cmp.compare(baseline, target)

        types_by_id = {d["device_type_id"]: d for d in result["device_types"]}
        self.assertTrue(types_by_id[74]["skipped"])
        self.assertIn("baseline", types_by_id[74]["reason"])

    def test_zero_baseline_kb(self):
        baseline = make_snapshot("emu", [make_device_type(kb=0.0, docs=0.0)])
        target = make_snapshot("real", [make_device_type(kb=100.0, docs=200.0)])
        result = cmp.compare(baseline, target)

        dt = result["device_types"][0]
        self.assertIsNone(dt["kb_ratio"])

    def test_fixture_files(self):
        """Compare the actual fixture files."""
        baseline = load_fixture("preprod_sample.json")
        target = load_fixture("uat_sample.json")
        result = cmp.compare(baseline, target)

        # Dove should be compared, Sentinel should be skipped (only in baseline)
        types_by_id = {d["device_type_id"]: d for d in result["device_types"]}
        self.assertFalse(types_by_id[24]["skipped"])
        self.assertTrue(types_by_id[74]["skipped"])

        # Real Dove devices have higher volume than emulators
        dove = types_by_id[24]
        self.assertGreater(dove["kb_ratio"], 1.0)

    def test_weighted_ratio(self):
        baseline = make_snapshot("emu", [
            make_device_type(dt_id=24, count=1000, kb=50.0),
            make_device_type(dt_id=74, name="Sentinel", count=10, kb=30.0),
        ])
        target = make_snapshot("real", [
            make_device_type(dt_id=24, count=100, kb=100.0),
            make_device_type(dt_id=74, name="Sentinel", count=5, kb=90.0),
        ])
        result = cmp.compare(baseline, target)

        # Weighted ratio should lean toward dt=24 (1000 devices vs 10)
        weighted = result["summary"]["weighted_kb_ratio"]
        self.assertIsNotNone(weighted)
        dove_ratio = 100.0 / 50.0  # 2.0
        self.assertAlmostEqual(weighted, dove_ratio, delta=0.1)


if __name__ == "__main__":
    unittest.main()
