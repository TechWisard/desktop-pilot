# Concurrent UI locale stability — 2026-11-07

## Observed failure

[Windows 2025 run 42393767131](https://github.com/TechWisard/desktop-pilot/actions/runs/42393767131) ran 525 Rust tests in draft organizer integration and failed exactly one production-egui assertion: `title_close_and_one_confirmation_stay_visible_at_normal_and_double_scale` could not find the Russian heading `Навести порядок` at 100%. The same commit passed this test on the Windows 2022 and Windows 11 ARM CI jobs. This is a recorded intermittent test failure; it does **not** prove that the visible production window permanently lost its title.

The code inspection found a shared process-global `AtomicBool` as the current language for both static labels and translated messages. Multiple test threads call `i18n::set` while rendering independent UI fixtures, and therefore can change each other's painted labels between frames. A second Settings or tray thread using another selected language has the same theoretical cross-thread interference. This is a concrete race in language state, not a timer-delay theory.

## Correction and regression

Every thread that explicitly calls `set` now retains its own UI language. The existing atomic language is kept as a fallback for background threads that have not chosen one; translations, persisted settings and user text are not changed. `simultaneous_windows_keep_their_own_visible_language` synchronizes two threads after one selects Russian and the other English. Both must then render static and runtime translations correctly. With only the former global flag, the two simultaneous assertions cannot both succeed.

The original bilingual organizer header/Close/confirmation paint and click tests remain enabled. They also serve as integration regression. No language selector, telemetry, disk file, history or desktop icon is modified.

## Acceptance

Require full Windows 2022 / 2025 / Windows 11 ARM Rust tests, strict Clippy, formatting, native UI/Explorer fixtures and existing release gates. This is a draft 0.3.0 integration correction, **not** authorization to publish the organizer. Physical multi-display, OneDrive, live interrupted-operation and keyboard-only testing remain outstanding.
