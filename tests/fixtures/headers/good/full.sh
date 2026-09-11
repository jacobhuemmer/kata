#!/bin/sh
# ---
# about: Trigger a branch pipeline
# risk:  medium
# needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token
# args:
#   branch: text = dev  # Branch, tag, or PR to trigger
#   version: text = ""  # Falls back to branch when blank
#   send_email: bool = true  # Send email on completion
#   k8s_context: select dev|uat|prod = dev  # Target environment
#   retries: int = 3
# alias: deploy quick-deploy
# timeout: 10m
# ---
#
# Usage notes: run this from a checked-out branch.
#
set -eu
echo "trigger"
