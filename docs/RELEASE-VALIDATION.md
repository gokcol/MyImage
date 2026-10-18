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

## Distribution

- The macOS update helper verifies the app identifier, distributor signature, newer build number, staging bytes, and installed-app rollback. It refuses running apps, preserves data/settings and restores the old app on failed verification.
- Production signing uses Developer ID, hardened runtime and trusted timestamp. Notarization must be Accepted, stapled and validated before the Mac artifact is published.
- Windows packages are built on GitHub Actions and marked **UNTESTED**. Compilation/unit checks do not establish interactive Windows capture, hotkeys, clipboard, scaling or installation behavior. No Windows code-signing certificate is configured.

Final local checks, notarization status and release links are recorded in the GitHub release notes. Undo history before an unexpected exit is not restored; edits after the last completed autosave can be lost. Multi-monitor capture, very large projects and physical Windows workflows require further real-device coverage.
