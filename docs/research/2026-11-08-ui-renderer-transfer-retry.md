# Verified UI renderer transfers — 2026-11-08

## Confirmed failure

PR #242 Windows 2025 [job 42464924695](https://github.com/TechWisard/desktop-pilot/actions/runs/42464924695) passed 533 Rust tests, Clippy, build, native integration and live Explorer acceptance, but its UI screenshot step stopped at a pinned Mesa distribution download with HTTP 504. This establishes a network dependency failure, **not** a native UI failure. The same PR head also passed a separate [push run 42464921687](https://github.com/TechWisard/desktop-pilot/actions/runs/42464921687) with both Windows 2025 and Windows 11 ARM UI reviews.

## Mitigation and acceptance

The existing app-local renderer URL and expected SHA-256 stay unchanged. The download helper makes at most three requests, deletes incomplete bytes between attempts, verifies the full SHA-256 before extraction, fails immediately for an incorrect checksum and still fails the UI gate after retry exhaustion. Neither renderer integrity nor mandatory Windows checks are skipped. The ordinary download test injects deterministic synthetic 504 failures and corrupt bytes through this same helper; it uses no paid services, private code or external traffic.

A successful transfer test does not prove that an external CDN will always respond, that native rendering will pass, or that a physical Windows 11 multi-display setup behaves correctly. Exact-head Windows checks and the original live UI review remain mandatory.

## Same-asset API route after exhausted direct downloads

The exact patch head `ddedd006` passed the synthetic transfer guard on all three Windows workers, but [42467132924](https://github.com/TechWisard/desktop-pilot/actions/runs/42467132924) hit HTTP 504 on all three direct-source attempts on Windows 2025. The asset's GitHub release metadata identifies REST asset `609112860` with the **same SHA-256** (`351fc8c8b695878ffb3eaa044b3ead08672a48b1a045e3c3e3975811df0f6695`) as the existing pinned download; [release 26.2.4](https://github.com/pal1000/mesa-dist-win/releases/tag/26.2.4). The helper now tries the original direct URL first, then the official release asset API route with `Accept: application/octet-stream` for two bounded attempts. All downloaded bytes still require the original digest before extraction, and an API metadata JSON response cannot pass verification. The deterministic transfer guard asserts the original-to-alternate request sequence. This alternative is a reliability hypothesis; actual CI may still fail if upstream access is unavailable. No checks are skipped.
