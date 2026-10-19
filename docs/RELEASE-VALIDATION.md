# Release validation — MyImage 1.0.0, build 40

MyImage is a Rust native macOS/Windows application. No WebView is used.

## Recovery and data safety

- Recovery writes the complete editable project atomically on a serial background worker, with a five-second delay and no continuous timer when unchanged.
- Autosave never overwrites manually saved projects or source images. Originals, editable annotations, geometry, groups and embedded assets use the normal `.myimage` format.
- Per-session OS locks prevent one running instance from recovering another instance's work. Incomplete temporary writes do not replace the last complete draft.
- Recovery offers Recover, Discard recovery and Decide later; deferred drafts remain reachable in the App menu.
- Manual project saving, undoing to a clean state and deliberate discard clear the active recovery copy. Exporting does not. Normal shutdown waits for queued writes and cleanup.
- Automated coverage includes an actual subprocess exit without destructors, editable round trips, lock exclusion, corrupt drafts, failed writes, bounded snapshot storage, stale asynchronous completions, save/export/undo semantics and shutdown ordering.
- Native macOS QA recovered a four-layer fixture, added an arrow, observed the autosave, simulated an unexpected exit of the isolated QA app, and recovered all five editable layers. A subsequent Save project cleared its recovery snapshot. Production artwork was not used.

## Performance and interaction

- Decoding, composition preparation and autosave encoding run off the UI thread.
- CPU opening cache: four files / 128 MiB. Source texture reuse cache: three files / 64 MiB, keyed by immutable version-checked pixel identity. Both release cached resources when hidden.
- Tests verify warm opens reuse the existing texture, replaced data gets a fresh texture, and cache eviction remains bounded.
- The compact first-run guide covers Capture, Annotate and Save/share, includes a platform-aware shortcut reference, persists dismissal, and is available later from the App menu.

## Verified checks

- Rust 1.93.1 is pinned for local builds and GitHub Actions.
- Local workspace tests: 406 passed (391 application, 15 packaging); 16 explicit diagnostic/integration tests ignored. Strict Clippy and formatting checks passed.
- The native guide was checked with its shortcut section expanded; its body scrolls while dismissal stays visible, and Escape dismisses it.
- A 3200 × 2000 headless opening benchmark reused prepared data and textures on warm opens. These measurements verify cache behavior, not complete native display latency.

## Distribution

- The macOS update helper verifies the app identifier, distributor signature, newer build number, staging bytes, and installed-app rollback. It refuses running apps, preserves data/settings and restores the old app on failed verification.
- Production signing uses Developer ID, hardened runtime and trusted timestamp. Notarization must be Accepted, stapled and validated before the Mac artifact is published.
- Windows packages are built on GitHub Actions and marked **UNTESTED**. Compilation/unit checks do not establish interactive Windows capture, hotkeys, clipboard, scaling or installation behavior. No Windows code-signing certificate is configured.

Final local checks, notarization status and release links are recorded in the GitHub release notes. Undo history before an unexpected exit is not restored; edits after the last completed autosave can be lost. Multi-monitor capture, very large projects and physical Windows workflows require further real-device coverage.

## Final distribution verification

- Apple accepted the application submission `f09b8f44-10bf-4c32-8eee-c83ab066b180` and DMG submission `d6931d53-f691-4df5-9b9f-7c77d5ec77da`. Both tickets were stapled and validated; Gatekeeper reports Notarized Developer ID.
- The transactional installer installed build 40 in Applications and verified a rollback of the actual installed build 39. Captures and projects were not changed.
- A regression test exercises codesign’s inline requirement parsing against an Apple-signed executable and rejects a mismatched identifier.
- [GitHub Windows build](https://github.com/gokcol/MyImage/actions/runs/39164925544) succeeded. The downloaded ZIP’s SHA-256, x64 PE header, UNTESTED notice and license files were verified. No Windows runtime validation was performed.
