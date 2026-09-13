#!/bin/sh
# ---
# about: Disk usage of a directory
# risk:  low
# args:
#   target_dir: text = .  # Directory to measure
# ---
du -sh "${TARGET_DIR}"
