#!/bin/sh
# ---
# about: Resolve a host
# risk:  low
# args:
#   host: text = localhost  # Host to ping
# ---
ping -c 1 "${HOST}"
