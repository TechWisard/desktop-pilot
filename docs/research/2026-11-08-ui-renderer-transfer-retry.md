# Verified UI renderer transfers — 2026-11-08

## Confirmed failure

PR #242 Windows 2025 [job 42464924695](https://github.com/TechWisard/desktop-pilot/actions/runs/42464924695) passed 533 Rust tests, Clippy, build, native integration and live Explorer acceptance, but its UI screenshot step stopped at a pinned Mesa distribution download with HTTP 504. This establishes a network dependency failure, **not** a native UI failure. The same PR head also passed a separate [push run 42464921687](https://github.com/TechWisard/desktop-pilot/actions/runs/42464921687) with both Windows 2025 and Windows 11 ARM UI reviews.

## Mitigation and acceptance

The existing app-local renderer URL and expected SHA-256 stay unchanged. The download helper makes at most three requests, deletes incomplete bytes between attempts, verifies the full SHA-256 before extraction, fails immediately for an incorrect checksum and still fails the UI gate after retry exhaustion. Neither renderer integrity nor mandatory Windows checks are skipped. The ordinary download test injects deterministic synthetic 504 failures and corrupt bytes through this same helper; it uses no paid services, private code or external traffic.

A successful transfer test does not prove that an external CDN will always respond, that native rendering will pass, or that a physical Windows 11 multi-display setup behaves correctly. Exact-head Windows checks and the original live UI review remain mandatory.
