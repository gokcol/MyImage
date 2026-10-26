# MyImage — handover for a fresh Codex session

Prepared 24 October 2026 (Europe/Istanbul); added to Git at the user's request on 25 October 2026. The user requested this file before deleting the previous chat. Read it as historical project context; inspect current files, Git state and UI before taking new actions. Do not repeat the completed release or cancel certification merely because those operations are described here.

Project root updated on 26 October 2026 (Europe/Istanbul) to `/Users/gokcol/SWPROJECTS/Utility/MyImage`. Use this directory for subsequent work; release evidence may retain historical workspace paths.

<!-- CURRENT_RELEASE_START -->
## Current outcome — 26 October 2026

Project: `/Users/gokcol/SWPROJECTS/Utility/MyImage`, branch `main`. The old workspace path is stale; pass the new directory explicitly to shell commands.

| Item | Latest verified state |
| --- | --- |
| Released source | `b1e07218b2f14cf388c4ac28754a0b047bfe2514` |
| App | **2.1.2 build 76** |
| GitHub | Public and latest, [v2.1.2-build76](https://github.com/gokcol/MyImage/releases/tag/v2.1.2-build76), published 26 October 2026 at 20:23 Istanbul |
| Mac | `/Applications/MyImage.app` updated and reopened; source, installed app, ZIP and DMG payloads match |
| Mac signing | Developer ID signed, Apple notarized, stapled, Gatekeeper accepted for app and DMG; build 75 rollback retained |
| Windows | x64 Setup and portable ZIP published; unsigned; installer lifecycle passed |
| Store package | New **2.1.4.0** verified, ready for upload |
| Store submission | **Not yet updated to build 76**: saved/Validated **2.1.3.0** remains In draft; the Mac screen locked before the new upload |
| Remaining work | User asked to unlock Mac; then upload 2.1.4.0, save prepared listing/reviewer notes and submit for certification |

The user reproduced main-canvas flicker by capturing a window, adding an arrow, and both dragging/reshaping it and changing color/width/style. Contained gestures now retain the original screenshot texture; canonical autosave preserves workspace brightness while input remains blocked. Real canvas expansion retains its fixed sampling frame until release. Build 76 also includes build 75's **Ctrl+Shift+C** capture shortcut on macOS/Windows and remembered per-tool styles/colors across reselection, images and restarts. Existing capture shortcuts remain.

Formatting, strict Clippy and **542 tests** passed (**527 application + 15 packaging; 23 ignored**). Persistent-frame regressions cover arrow draw/move/resize/bend/style, boundary crossing, saving appearance and blocked/resumed edits. Independent code and artifact reviews passed. All four exact-source workflows passed: native macOS `40365679103`, Windows installer `40366073789`, real Store `40366078963`, test MSIX lifecycle `40365679085`. Interactive Windows capture/clipboard/device validation remains pending.

Mac notarization IDs: app `9a0c4f67-c62e-44c7-91fa-f370064a6178`, DMG `ee9a8141-47a9-47d0-be7b-0a95e04f8e68`, both Accepted. The existing **MyPDF** Keychain profile initially returned a transient lookup failure, then authenticated and completed both submissions without credential changes. Reuse it; no replacement credentials were created.

Current evidence:

- `dist/release-v2.1.2-build76/release-validation.json`, `macos-validation.json`, release notes, signing/notarization/install/check logs.
- `dist/windows-v2.1.2-build76/windows-validation.json` and artifacts.
- `dist/msix-v2.1.2-build76/artifact-validation.json` and **MyImage-2.1.4.0-store-x64.msix** (SHA-256 `3634301a5fd4a7a8573e6931dd40479054eda77b12717dcc5087a87060d26619`).
- Store text prepared in `dist/release-v2.1.2-build76/store-whats-new.txt` and `store-reviewer-notes.txt`. Existing relevant reviewer instructions are preserved in the merged text. Read/recheck live state before resuming.

Store update/final submission is already authorized. Product **9PL3HWB23J38**, submission **1152921505702179656**, identity `OrhanGkl.MyImageOG`, publisher `CN=A9F0208B-CC43-472D-AE82-9FE4556DC935`. Preserve pricing/markets/ratings/capabilities/startup and publish-after-approval preferences. Firefox native UI automation cannot operate while the Mac screen is locked; do not alter security settings. If the upload link cannot open the chooser, the user previously opened it manually, after which exact-path selection worked. Do not claim Microsoft signing or availability before certification. Update published `release-validation.json` with the final observed Store state after submission.

Build 75 remains a historical public release. Its **2.1.3.0** package replaced 2.1.2.0, but was held In draft for this flicker fix. Do not overwrite prior release tags or binaries. The unrelated deletion `assets/branding/myimage-calico-logo-v1.png` remains deliberately unstaged.

The sections below retain historical build-74 details and operating procedures. Use this current block and release reports for current versions and status.
<!-- CURRENT_RELEASE_END -->

## Historical build 74 outcome

The latest requested work is complete: the mirrored-display capture fix was committed, pushed, released for macOS and Windows, installed on the Mac, and submitted to Microsoft Store.

| Item | Last verified state |
| --- | --- |
| Repository | `/Users/gokcol/SWPROJECTS/Utility/MyImage`, branch `main`, `gokcol/MyImage` |
| Released source | `a6c95c0ec2ddc255cfa570b7aec8192582182fec` — `Resolve mirrored displays automatically during capture` |
| App version | **2.1.0**, **build 74** |
| GitHub release | **Public**, tag `v2.1-build74`, marked latest |
| Release URL | <https://github.com/gokcol/MyImage/releases/tag/v2.1-build74> |
| Installed Mac app | `/Applications/MyImage.app`; About verified **2.1.0 · build 74**; opened successfully |
| Mac signing | Developer ID signed, Apple notarized, stapled, Gatekeeper accepted; both app and DMG |
| Windows downloads | x64 Setup installer and portable ZIP published; **unsigned** |
| Store package | **2.1.2.0**, `MyImage-2.1.2.0-store-x64.msix`, uploaded and validated |
| Store submission | Replaced pending 2.1.1.0; resubmitted; **In certification / Pre-processing**, auto publish after approval |
| Store status checked | 24 October 2026, about 02:32 Istanbul time; this is a snapshot, not a live status |

No required implementation or release step remains from that request. Microsoft approval is external. Check its present status when requested; do not claim the product is live based on the certification snapshot. A full interactive Windows capture/clipboard/device review was not performed for build 74; installer lifecycle and package checks did pass.

## Preserve the working tree

At handover preparation, the only pre-existing working-tree change was:

```text
 D assets/branding/myimage-calico-logo-v1.png
```

This unrelated deletion was deliberately excluded from the release commit and the handover commit. Do not restore, stage or remove anything else to “clean up” this change without understanding the user's intent. This handover is a documentation-only addition after the release; the build-74 release tag still identifies the source commit above. No application source was changed for the handover.

## Release evidence and files

The best current evidence is in these files, also attached to the public release:

- `dist/release-v2.1-build74/release-validation.json` — source SHA, artifact hashes, notarization IDs, Mac installation/capture check, workflows and Store status.
- `dist/release-v2.1-build74/windows-validation.json` — installer/package verification and exact-source CI results.
- `dist/release-v2.1-build74/release-notes.md` — published release notes.
- `dist/windows-v2.1-build74/validation.json` — original Windows agent report; its draft asset URLs were corrected in the published `windows-validation.json`.
- `dist/msix-v2.1-build74/artifact-validation.json` — independent Store identity/version/hash verification.

Local release directories:

```text
dist/release-v2.1-build74/
  MyImage-macos-aarch64.zip                 # notarized app, canonical download name
  MyImage-2.1.0-build74-macos-arm64.dmg
  corresponding .sha256 files
  bundle.log, notarization.log, dmg.log, install.log
  release-notes.md, release-validation.json, windows-validation.json
dist/windows-v2.1-build74/                  # Setup, portable ZIP, checksums, reports
dist/msix-v2.1-build74/
  MyImage-2.1.2.0-store-x64.msix
  corresponding .json and .sha256 files
target/bundle/MyImage.app
target/bundle/MyImage-macos-aarch64.zip
backups/                                  # verified rollback retained by packaging/install helpers
```

All four exact-source GitHub workflows passed:

- Native formatting, strict Clippy and tests: <https://github.com/gokcol/MyImage/actions/runs/40039010438>
- Windows installer/portable and lifecycle checks: <https://github.com/gokcol/MyImage/actions/runs/40039210475>
- Real Store MSIX: <https://github.com/gokcol/MyImage/actions/runs/40039214047>
- Disposable test-identity MSIX install/remove: <https://github.com/gokcol/MyImage/actions/runs/40039010455>

**528 tests passed: 513 application + 15 packaging**. There were 23 explicitly ignored tests; do not describe those as executed. Installer checks covered install, upgrade, uninstall, project preservation, binary identity, shortcuts, file-open quoting and startup defaults.

## Latest fix: mirrored displays

The user saw “The selected display is no longer available” on an external mirrored display and wanted automatic handling like Snagit, without supplying an external display name.

- `src/capture.rs`: `resolve_display` returns fresh `DisplayInfo`, preferring exact IDs. On macOS, `CGDisplayMirrorsDisplay` identifies equivalent members of the same mirror family. ScreenCaptureKit matching also uses that relationship. Zero IDs, unrelated screens and missing displays are rejected; there is no arbitrary “use the sole remaining screen” fallback. Windows retains exact ID matching.
- `src/app.rs`: the capture worker re-enumerates and resolves the selected display **after the hide/timer delay**, updates the display list and passes fresh geometry to the overlay. Demo behavior and cancellation/stale-result guards remain intact.
- Tests cover exact preference, forward/reverse/sibling mirror aliases, invalid/unrelated IDs and fresh geometry. An independent review found no correctness issues.
- On the connected mirrored hardware, native metadata verification resolved built-in display **1** to desktop/source **2**, bounds **2560 × 1440**. The installed build then successfully opened the real region-selection overlay. It was canceled before saving an image. This verifies capture startup; it is not a saved-image/export regression for every monitor arrangement.
- Test name for the opt-in metadata-only check: `native_mirror_members_resolve_to_the_current_desktop_without_capturing_pixels`.

The preceding release, build 73 (`ab91199`), fixed Recents flicker after canvas resize/autosave: retain existing previews, replace only the edited thumbnail, reject stale asynchronous replies, and preserve Library selection/scroll. Keep this behavior intact.

## Product decisions and user preferences

The user wants a polished, compact, quick capture/editor experience comparable to Snagit. Avoid making panels or controls larger. They prefer action and concrete reviewable changes over repeated confirmation questions, while unsaved work and actual tool-required approvals still need protection.

- **Preserve established icon/control positions** during appearance changes. Cobalt Satin is the chosen editor finish; Amber Enamel and Jade Graphite remain selectable in Settings.
- Icon styles remain selectable: Studio Color, Cobalt Duotone and Precision Outline. Keep Standard and subtle Calico pointer families selectable, with appropriate precision/text/grab/resize equivalents; pointer artwork must not appear in exports.
- **Capture Now** uses the selected **Pearl Compact** design, subsequently refined to a lighter graphite background and a large **red circular Capture button**. Native size is 560 × 292 logical points; independent of editor theme. Keep options compact and aligned.
- Pen is pinned immediately after Shapes. Overflow is a compact borderless chevron, not a large “More” button; the redundant main-toolbar Copy group was removed. Copy shortcuts and context actions remain.
- Text and callouts edit **in place** with aligned compact controls, rather than a large modal editor. There are twelve callout styles and a main-toolbar Magnify tool with twenty visual presets/linked source areas.
- Arrow/shape floating controls show only essential quick properties: color, slim vertical stroke-weight slider, line pattern, endpoints or fill/outline. Advanced controls stay in the inspector. Do not make popups obscure the selected drawing or duplicate the entire inspector.
- Library/date rows and menus are compact. Recents uses a stylish disclosure control without a “Recents” text label. The right inspector is resizable, starts tightly fitted, and supports double-click/keyboard reset.
- Settings is compact. No manual “Refresh status” startup button: query when opening Settings, returning from system settings or changing the option.
- Background mode remains recoverable through Dock/Finder or Open Editor in the menu bar/tray. **Start at login is opt-in, default off**, including Store startup task. Updates must not enable startup automatically.
- Pen/finger touch has fixed-width freehand support. Pressure-varying width, palm rejection and actual pen/touch hardware validation were not added/performed.

## Project-first storage — important invariant

The user explicitly rejected managing a captured PNG and a separate edited project manually.

- Opening/importing an external image, taking a capture into the editor, pasting, or creating a blank canvas/template creates a canonical editable **`.myimage` project automatically**. Embedded source pixels and edits travel together.
- **Library and Recents show projects only**. PNG/JPEG/etc. are explicit flattened exports, not duplicate project entries. Export must not add duplicate browsing entries.
- External imported originals stay untouched. Legacy managed capture/project pairs are matched by original pixels; only verified managed dated captures are retired to Trash after durable association/conversion. Do not bulk-delete external originals.
- Deleting from Library/Recents trashes the project and its remaining **managed** image companion; embedded originals go with the project. Recents also has a distinct non-destructive “Remove from Recents” action.
- Project saves flush before navigation/quit. Keep recovery, no-clobber allocation, migration idempotence, undo and deletion safeguards intact.
- Relevant code: `src/project_store.rs`, `src/project_session.rs`, `src/document.rs`, `src/storage.rs`, `src/library*.rs`, `src/recent_history.rs`, `src/recovery*.rs`.

## Capture and OCR behavior

- Region/window capture still defaults to confirmation so the user can adjust/change type. Optional capture-on-release is selectable and **off by default**.
- Multi-region combines selected areas in their original positions with transparent gaps. Finish explicitly even with immediate capture enabled.
- Scrolling capture is **guided, manual, vertical**. User selects an area/window, starts scrolling, then scrolls with overlap. Continuous multi-page PDFs are supported; this is not automatic scrolling or horizontal stitching. Build 69 fixed collecting only the first view. No-extra-section completion should explain the issue and offer recovery, not silently output the first page.
- **Extract text must be available whenever an image exists**: no pixel selection means whole visible image; selection means that region. Review is editable and Copy is explicit.
- Capture Now **Text** mode recognizes a screen region and copies text directly without opening the editor or saving a project. Cancel/failure preserves the clipboard.
- First-use OCR hanging was fixed; keep background execution, cancellation and time limits. On-device Apple Vision / Windows OCR services are used. Windows languages depend on OS support. Native Windows OCR passed earlier; build 74 did not rerun the unchanged OCR workflow.
- Relevant code: `src/scroll_capture.rs`, `src/capture_session.rs`, `src/capture_output.rs`, `src/screen_text.rs`, `src/text_extract.rs`, `src/ocr/`, `src/selection_panel.rs`.

## Source orientation and validation

This is a **native Rust/egui/winit/wgpu app**, not a web app. Rust is pinned to **1.93.1**. Supported targets are macOS 14+ and Windows.

- App/workers/state: `src/app.rs`, `src/runtime.rs`, `src/desktop.rs` and `src/desktop/`.
- Editor/canvas/annotations: `src/editor.rs`, `src/canvas.rs`, `src/document.rs`, `src/object_properties.rs`, `src/render.rs`, related split modules.
- Appearance/settings: `src/appearance.rs`, `src/icon_style.rs`, `src/pointer_style.rs`, `src/pointer_art.rs`, `src/app_menu.rs`, `src/dialogs.rs`, storage settings. There is **no `src/settings.rs`**.
- Capture launcher: `src/capture_launcher.rs` (included module), `src/capture_panel.rs`, `src/capture.rs` and session/output modules.
- Packaging/install: `xtask/src/main.rs`, `xtask/src/install.rs`, `xtask/src/cleanup.rs`, `scripts/`, `packaging/`, `.github/workflows/`.
- `docs/RELEASE-VALIDATION.md` is the chronological implementation/testing record; consult the latest section and release JSON for current facts. Historical paths and test counts can be obsolete.
- **README caveat:** its top download link still points to 2.0 build 68, and some old capture/autosave paragraphs predate project-first storage. Local Store-listing text files also include old 2.0 copy. The actual current GitHub release and live Store build-74 notes were updated. Documentation cleanup is a possible future task, not unfinished release execution.

Normal source checks (run once as appropriate to a future change):

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

`--demo` / `MYIMAGE_DEMO` uses isolated generated content and temporary storage. Prefer an isolated uniquely identified review app for destructive/native editing tests. A demo capture cannot validate the real ScreenCaptureKit path. The Windows native OCR workflow is `.github/workflows/ocr.yml`; use it when OCR changes warrant it.

## Mac signing, packaging and installation

Reuse the existing Developer ID identity and Keychain profile; do not generate replacement certificates or ask for passwords in chat.

```sh
MYIMAGE_DISTRIBUTION=1 \
MYIMAGE_SIGNING_IDENTITY=1C4AC64B0DA94690712849B0A916B1D36FB57BA8 \
cargo run --locked -p xtask -- bundle

target/debug/xtask notarize MyPDF

MYIMAGE_SIGNING_IDENTITY=1C4AC64B0DA94690712849B0A916B1D36FB57BA8 \
MYIMAGE_NOTARY_PROFILE=MyPDF \
sh scripts/package-dmg.sh target/bundle/MyImage.app dist/<new-release>

# After checking the installed app's document is safe and quitting normally:
target/debug/xtask install
```

- Identity fingerprint is public configuration, not a password; team **HXDYTBC6ZT**, Orhan Gökçöl. Profile **MyPDF** was available and worked for build 74. Keychain credentials were never written into the repository.
- Build 74 app notarization: `8a06a761-c62e-4c1d-9ee4-15a02c861b33`, Accepted. DMG: `7bc4ed78-2ca8-45aa-864c-2c282e491a8a`, Accepted.
- `packaging/macos/Info.plist` contains version/build. Cargo version is 2.1.0. Bump the build for future releases; do not overwrite build 74 artifacts/tag.
- Bundle/install helpers verify signing and retain rollback. Successful release packaging cleans build caches/obsolete disposable apps, so the next compile may be cold. Avoid concurrent competing release jobs and avoid `cargo clean` when evidence is needed.
- Do not modify a signed bundle afterward, install ad-hoc/unnotarized builds as the regular app, bypass Gatekeeper or reset TCC as routine maintenance. Preserve the signing requirement and capture permissions.
- `.github/workflows/macos-dmg.yml` exists but requires separately configured signing/notary secrets. Build 74 was signed/notarized locally; do not assume hosted Mac signing is configured.

## Windows and GitHub release procedure

Public Windows downloads are intentionally **unsigned** under the user's decision to use **Microsoft Store signing after certification**. No external signing service was configured. Never claim GitHub Setup.exe is signed. The MSIX submission artifact is also unsigned and is not an end-user installer.

- Setup uses Inno Setup; relevant files are `packaging/windows/MyImage.iss`, `scripts/package-windows.ps1`, `scripts/test-windows-installer.ps1`.
- Store uses `scripts/package-msix.ps1`, `scripts/test-msix.ps1`, `scripts/test-msix-install.ps1`, `packaging/windows/msix/`.
- `packaging/windows/msix/version.txt` currently reads **2.1.2.0**. Store version must increase monotonically and end with `.0`; it is independent of the app build counter.
- Prefer a manually pushed annotated release tag at the exact source SHA, then create the GitHub draft, dispatch workflows, attach Mac files/checksums, verify all artifacts, and publish. Use a full SHA, not an abbreviated SHA, when a GitHub API requires a target commit. A draft release alone is not proof the tag was pushed.
- GitHub CLI network/auth may need execution outside the sandbox; a sandbox failure does not prove the token is invalid. Do not print credentials.

For a **future** tag/package version, adapt these commands rather than rerunning build 74:

```sh
gh workflow run windows-release.yml --repo gokcol/MyImage --ref main \
  -f release_tag=<new-tag>
gh workflow run windows-msix.yml --repo gokcol/MyImage --ref main \
  -f ref=<new-tag> -f mode=store -f package_version=<new-store-version>
```

Verify exact source SHA, PE architecture, version, Store identity and hashes, installer lifecycle and package checks. Attach Setup, portable ZIP, DMG, Mac ZIP, checksums and validation evidence before publishing. Current checksums are in `release-validation.json`; no need to recompute or rebuild merely to resume this project.

## Microsoft Partner Center

Public identity values (do not substitute a newly guessed identity):

```text
Product / reserved name: MyImage (OG)
Product ID:             9PL3HWB23J38
Submission ID:          1152921505702179656   # current Submission 1; verify for future work
Package Identity Name:  OrhanGkl.MyImageOG
Publisher:              CN=A9F0208B-CC43-472D-AE82-9FE4556DC935
Publisher display name: Orhan Gökçöl
Store package version:  2.1.2.0
```

Overview: <https://partner.microsoft.com/en-us/dashboard/products/9PL3HWB23J38/overview>

Known current routes:

```text
.../products/9PL3HWB23J38/submissions/1152921505702179656/packages
.../products/9PL3HWB23J38/submissions/1152921505702179656/listings?languageid=4&languagecode=en-us
.../products/9PL3HWB23J38/suppinfo/additionaltestinginfo
```

The listing is free, available in all supported markets, and has existing Everyone / 3+ ratings. The reserved name's earlier missing parenthesis was corrected. English description reflects 2.1/project-first/local OCR; What's new names build 74; reviewer notes identify MSIX 2.1.2.0 / app 2.1.0 build 74. Those were saved, not just drafted. Previous 2.1.1.0 was removed on Save; only new 2.1.2.0 was shown as Validated before submission. Current declared desktop capabilities `runFullTrust` and `unvirtualizedResources` were unchanged; their warnings are certification review requirements, not package-identity errors.

The user authorized the completed update/resubmission and previously accepted IARC terms. Do not accept a new legal agreement based solely on this document; follow current tool policy and user authorization. Do not restart ratings or alter markets/pricing casually.

## Native UI automation lessons

Use the current available CUA tool/documentation for UI work. The old chat's JavaScript bindings, element indices, browser state and subagents will not carry over. No browser or UI state described here is live.

- Firefox native automation worked for Partner Center; Firefox DOM control was unavailable. Existing-account selection resumed its session. If interactive authentication is required, hand it to the user rather than handling a new password or bypassing security.
- Avoid inspecting unrelated private browsing content. Navigate to the known Partner Center page/tab before reading its page. Respect actual user interaction and refresh state after it.
- After actions, get fresh accessibility state and derive new IDs. A full accessibility snapshot can renumber every control. Never reuse an ID from before that refresh.
- Clipboard paste followed immediately by Return sometimes navigated to the old URL. Verify navigation; using a settable address/path field with a separate observation before Return is more reliable.
- Upload chooser: open it, then **in a separate call** Cmd+Shift+G; set the path field to the exact `.msix`, inspect it, Return, verify the selected file is **MyImage** with the correct version, then Open. The chooser may initially point to a MyPDF artifact directory; never select it by assumption.
- Partner Center sometimes kept “Analyzing package” and Save disabled after backend validation. Once the new version row and automatic old-package-removal message were present, reload/open a fresh view of the exact Packages URL. After load, Save became enabled. Save, verify new package **Validated** on overview, submit, then verify **In certification**. An attempted click or upload progress is not completion.
- Get MyImage by explicit path `/Applications/MyImage.app` because multiple review bundles can share its app identifier. Read/check saved document state before closing for installation. Quit normally, then install; querying the app immediately after quit can relaunch it.
- After starting real capture, do independent work before querying MyImage; an immediate query during its short hide/compositor delay can re-show/cancel it. Check the selection overlay through AX and cancel a test before saving private screen content.
- Parallels previously failed to forward automated pointer/keyboard input into Windows even when the guest itself worked. Do not conflate that automation limitation with an application bug. Prior Store screenshot required user help; current release relied on hosted Windows validation.

## Suggested first message in the new chat

> Continue MyImage in `/Users/gokcol/SWPROJECTS/Utility/MyImage`. Read `docs/HANDOVER.md` first, then inspect the current Git status. Build 74 is already released and installed; Store 2.1.2.0 was submitted and awaiting certification. Preserve the existing deleted branding PNG. My next task is: …
