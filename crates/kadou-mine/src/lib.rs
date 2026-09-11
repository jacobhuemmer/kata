//! Session mining engine: transcript ingest, per-agent parsing, normalize/cluster/rank,
//! redaction, and single-file draft proposals (`docs/design/05-prd.md` §6.8, §9 slice 9).
//!
//! Not yet implemented — lands in slice 9 ("Session mining (`kadou-mine`)") per §9. Carries
//! no MCP types; the miner is a local batch job driven by `kadou mine` and a LaunchAgent.
