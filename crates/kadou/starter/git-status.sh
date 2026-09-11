#!/bin/sh
# ---
# about: git status -sb in a repo
# risk:  low
# args:
#   target_dir: text = .  # Repo directory
# ---
git -C "${TARGET_DIR}" status -sb
