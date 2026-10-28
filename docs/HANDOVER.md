# MyImage — handover for a fresh Codex session

Prepared 24 October 2026 (Europe/Istanbul); added to Git at the user's request on 25 October 2026. The user requested this file before deleting the previous chat. Read it as historical project context; inspect current files, Git state and UI before taking new actions. Do not repeat the completed release or cancel certification merely because those operations are described here.

Project root updated on 26 October 2026 (Europe/Istanbul) to `/Users/gokcol/SWPROJECTS/Utility/MyImage`. Use this directory for subsequent work; release evidence may retain historical workspace paths.

## Version 2.2 release candidate — build 85, 28 October 2026

The final **v2.2** candidate is **2.2.0 build 85**, including all editor and Library improvements described below. Quick resize preserves the selected Fit/manual-zoom mode and briefly displays the new output dimensions on the canvas, making changes clear even when Fit keeps the same apparent image size. Undo/Redo buttons remain visible beside image controls; shortcuts work after menu, toolbar and property controls while text fields/modal drafts retain their own keyboard behavior. Image history survives autosave, including rotation and resize. Undo and Redo use the same action handler for buttons, menus and keyboard shortcuts.

Validation passed: **592 tests** (577 application + 15 packaging; 23 ignored), formatting, strict all-target workspace Clippy and Windows MSVC cross-compilation. Added regressions cover both view modes, feedback expiry, shortcuts after toolbar focus, modal-draft protection, and rotation/resize Undo/Redo across real autosaves. Native Mac review confirmed the temporary 660×660 indicator and the visible Undo/Redo buttons; after a saved rotation, Undo restored horizontal text and Redo restored the rotation. Physical keyboard automation was not sent while Firefox retained foreground; event-level UI tests cover the shortcuts.

Evidence: `dist/local-mac-v2.2.0-build85/` and `dist/release-v2.2-build85/`. Signing, notarization, installation, Windows installer verification and GitHub publication are being completed for this source. Build 84 is currently installed and was verified; its unpublished draft release was superseded by these final improvements. Store submission is unchanged. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 84 — 28 October 2026

The final **v2.2** release uses application version **2.2.0 build 84**. Canvas menu actions have contextual icons; **Rotate**, **Flip** and **Select** each use one compact icon row. **Select all annotations** has colored objects inside its selection outline. Inline minus/plus buttons change the current output size by 10% per click. The **Resize image** dialog contains the 25%, 50%, 125%, 150% and Set to original presets: they update the draft fields and require **Apply resize**; Cancel leaves the image unchanged. Command/Ctrl+Shift+R opens that dialog with canvas focus. Library thumbnails omit the PROJECT badge; per-day checkboxes preserve other-day selection and show all/partial/none states.

All **590 tests** pass (575 application + 15 packaging; 23 ignored), plus formatting, strict workspace Clippy and Windows MSVC cross-compilation. Native Mac review confirmed the final menu rows and colored selection icon, repeated 10% resize clicks (600 → 660 → 726), dialog preset dimensions (50% → 300, before Apply), Cancel, clean Library thumbnails and partial day selection (9 of 10). Evidence is in `dist/local-mac-v2.2.0-build84/`, including screenshots. Mac packaging, notarization and installation completed for build 84; its GitHub draft was superseded by build 85 before publication. Evidence remains in `dist/release-v2.2-build84/`. Windows interactive device testing remains pending. Store submission is unchanged.

Build 83 was installed and verified, but its draft release assets/source were superseded before publication by the user's final menu requests. Only the unpublished v2.2 candidate tag is being advanced to build 84; previously published release tags remain unchanged. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 83 — 28 October 2026

The user requested publishing a new **v2.2** GitHub release with the Windows installer. Application version is **2.2.0**, build **83**. This release includes the compact editor, remembered styles/colors, quick-style tabs/custom slots, text paths and tools delivered in builds 77–82, plus these final changes:

- Library date separators have checkboxes for all/partial/none selection. A day checkbox affects only its displayed group, including offscreen projects, and preserves other days. PROJECT badges are removed from thumbnails.
- Canvas context-menu actions have contextual icons. Inline minus/plus buttons resize the current output by 10% per click without closing the menu. Presets offer 25%, 50%, 125%, 150% and Set to original, based on the unscaled canvas and preserving crop/rotation. The detailed resize dialog opens from the menu or Command/Ctrl+Shift+R with canvas focus. Resize operations preserve source pixels/annotations and support Undo/Redo.

**2.2.0 build 83** is installed at `/Applications/MyImage.app`, opened and confirmed running. Existing Developer ID team/signing requirement are unchanged. Apple notarization `c9b69964-33c7-4f9e-b2e6-1eff30f62420` was **Accepted** using MyPDF; the ticket is stapled/validated and Gatekeeper reports **Notarized Developer ID**. Installed app/source bundle/ZIP match, rollback matches the previously installed build 82, and settings were byte-identical before first launch.

All **590 tests** pass (575 application + 15 packaging; 23 ignored), along with formatting, strict workspace Clippy, Windows MSVC cross-compilation and whitespace checks. Pointer tests cover day selection/partial states and repeated resize controls/presets/modal/keyboard shortcut. Native Mac review confirmed selecting all 10 projects for a day; further native visual checks stopped when the Mac locked, before thumbnail badge removal. Windows interactive device validation remains pending. Local evidence is in `dist/local-mac-v2.2.0-build83/`; release packaging/evidence goes in `dist/release-v2.2/`. This build was superseded before GitHub publication by build 84 above. Store submission is separate and unchanged. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 82 — 28 October 2026

Library now supports rectangle selection in both grid and list views. Drag from a project or empty gallery space; Shift adds to the previous selection and Command/Ctrl toggles against it. The gallery scrolls at the top/bottom while dragging. Escape restores the selection from before the drag. Filtering, refreshing or changing layout cancels an active drag, and Library selection remains separate from Recents. Existing click/range selection, double-click opening, context menus and Delete remain available.

**Select all** and **Deselect** appear beside the item count. Select all applies only to the selected day/month/year and current search/collection filter. Every project has a selection checkbox; clicking it adds/removes that project without opening it or clearing the other selected projects. Year and month filters include all projects in that interval. The gallery groups projects beneath year/month/day separators with per-day counts; the chosen sort applies within each day. Drawing and hit testing share the same virtualized date-row layout, so selection also works across separators and offscreen rows.

**2.2.0 build 82** is installed at `/Applications/MyImage.app`, opened and confirmed running. It uses the same Developer ID identity/team and unchanged signing requirement. Apple notarization `a91ca3c7-9888-4bae-84f4-da38e954d4f4` was **Accepted** using MyPDF; the ticket is stapled/validated and Gatekeeper reports **Notarized Developer ID**. Installed app/source bundle/ZIP match. Rollback is verified against the previously installed **2.2.0 build 81**, and settings were byte-identical before and immediately after installation, before first launch.

Validation: **585 tests** passed (570 application + 15 packaging; 23 explicitly ignored), formatting, strict all-target workspace Clippy, Windows cross-compilation and whitespace checks. New pointer tests cover grid/list dragging, Shift/Command/Ctrl behavior, keyboard batch actions, scroll to virtual rows, Escape, cross-date dragging, filter cancellation, checkboxes, and day/month/year/search bulk selection. Native Mac demo review verified year/month/day counts (15/13/3), Select all, Deselect, individual checkboxes, a two-project drag and multiple date separators with small thumbnails. Evidence is in `dist/local-mac-v2.2.0-build82/`, including native screenshots, source patch, validation logs, notarization receipt and signed/notarized ZIP. Windows native interaction and user testing remain pending. Source remains uncommitted on base `167f6a6a92d97497951dbe3cbc1085b2d652e42f`; no GitHub release or Store update was made. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 81 — 28 October 2026

This build adds the requested editor features while retaining the compact ribbon, panel pinning, quick-style tabs/custom slots, and remembered appearance/color settings.

- **Text on a path** works for new or existing text, with horizontal, vertical, diagonal, circle, square and editable custom paths. Text can sit Over/On/Below the path with adjustable distance, spacing and fit. Path visibility, color, opacity, width, size and rotation are independent. Custom nodes can be moved, added or removed; a new custom path can be drawn. Double-click path text to edit its wording and formatting. Preview and flattened export share glyph geometry. Project format advances to **23** for path/connection persistence.
- **Color Inspector** now has a canvas pixel magnifier and **Pick color anywhere on screen…**. On macOS the native magnified color picker works outside the canvas; Windows has an all-display picker with a magnifier. Canvas samples include the visible composition and discard stale results. Native Mac review confirmed picking a title-bar color outside the image. Windows compiled successfully; native Windows interaction remains untested.
- **More tools** adds Connected Arrows, Smart Erase / Repair, Edit Screenshot Text and Redact for Sharing. Connected arrows follow object movement/resizing and support straight, curved and elbow routes. Repair previews surrounding-color interpolation or a neighboring clone. Screenshot text uses local OCR and an editable text layer over a grouped repair patch; font and color are estimates with editable controls. Redact adds opaque covers and copies a flattened image; editable projects still contain source pixels. **Compare Images was not added.**
- **Arrow properties** removes the 1/2/4/8/12/24 width buttons and yellow-point help sentence. A **Routes** group holds Line, Arrow, Double Arrow, Elbow and Straighten together in one icon-only row, with tooltips and accessible names. Width remains on its slider.
- Selected artwork has a **Styles** icon in its floating quick actions. It opens Styles while preserving the selected objects, document and appearance settings.
- **Customize toolbar** can reorder every tool, including unpinned tools. Checkboxes choose ribbon visibility; arrows control one saved order used by the ribbon and More tools. Migration preserves the previous pinned order, and missing/new tools are appended. Order and pins survive settings serialization and document changes.

**2.2.0 build 81** is installed at `/Applications/MyImage.app`, opened and confirmed running for user testing. The existing Developer ID identity/team and designated signing requirement remain unchanged. Apple notarization `27546b92-259e-4048-84fa-f96569d149b5` was **Accepted** using MyPDF; the ticket is stapled/validated and Gatekeeper reports **Notarized Developer ID**. Installed app/source bundle/ZIP match. The verified rollback is **2.1.3 build 80**; settings were byte-identical before and immediately after installation, before first launch.

Validation: **581 tests** passed (566 application + 15 packaging; 23 explicitly ignored), formatting, strict all-target workspace Clippy, Windows cross-compilation and whitespace checks. Regressions cover text-path geometry/exports/project round trips; connector following, remapping and deletion; repair/OCR/redaction previews, source preservation and Undo; magnifier/screen sampling; compact Routes controls; pointer activation of the Styles shortcut; and persisted ordering of unpinned tools. Native Mac demo review checked path rendering, the canvas magnifier and outside-canvas picker, repair and editable OCR previews, redaction, Routes actions, the Styles shortcut and unpinned-tool reordering. The final customizer column alignment is covered by the UI tests; screenshots predate that one-line alignment adjustment. Evidence is in `dist/local-mac-v2.2.0-build81/`, including native screenshots, validation logs, source patch, notarization receipt, audit and signed/notarized ZIP. Source remains uncommitted on base `167f6a6a92d97497951dbe3cbc1085b2d652e42f`. User testing remains pending. No GitHub release or Store change was made. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 80 — 28 October 2026

Three new editor tools are available in **More tools**, using the existing compact Styles panel:

- **Style Brush** copies appearance between compatible annotations. Right-click an annotation → Style Brush picks that object immediately; More tools starts with source selection. Click targets repeatedly, Option/Alt-click to repick, or press Escape/Done to finish. Shapes share styles; other kinds match their own kind. Text/content, positions, arrow bends, lens source offsets, step numbers, masks and layer/group settings remain with the target. Locked targets are protected and each application supports Undo.
- **Measure** reads distance, width and height in output image pixels, independent of zoom and accounting for image transforms. Free/Horizontal/Vertical modes and Shift axis constraint are available. Copy measurement does not edit the document. Add dimension annotation creates a grouped editable line/text pair in one undo step; its label records the value at creation and does not automatically recalculate after later edits.
- **Color Inspector** samples the full visible export composition locally, including annotations and privacy effects. It copies HEX/RGB with alpha when needed, applies color to selected editable objects, and can remember the sampled color for a drawing tool. Sampling runs in the background and discards results if the image changed. Existing quick styles, custom slots, colors and toolbar positions are retained.

**2.1.3 build 80** is installed at `/Applications/MyImage.app`, opened and confirmed running for user testing. The existing Developer ID identity/team and designated signing requirement remain unchanged. Apple notarization `2f97ad40-7fb5-44f5-8f1b-ed9fe573d42a` was **Accepted** using MyPDF; the ticket is stapled/validated and Gatekeeper reports **Notarized Developer ID**. Installed app/source bundle/ZIP match. The verified rollback is **2.1.3 build 79**; settings were byte-identical before and immediately after installation.

Validation: **564 tests** passed (549 application + 15 packaging; 23 explicitly ignored), formatting, strict all-target workspace Clippy and whitespace checks. Twelve new regressions cover style/content/geometry separation, locked/incompatible targets, measurement transforms and cancellation, dimension project persistence/Undo, privacy-aware sampling and stale results, color application/defaults, exact clipboard strings, real pointer/menu interactions and canvas stability. Native demo review verified all three tools, right-click style transfer, sampled-color application, and measurement/addition at minimum window size. Evidence is in `dist/local-mac-v2.1.3-build80/`, including native screenshots, test logs, a source patch including the new module, notarization receipt, audit and signed/notarized ZIP. Source remains uncommitted on base `167f6a6a92d97497951dbe3cbc1085b2d652e42f`; user testing and Windows native interactive review remain pending. No GitHub release or Store change was made. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 79 — 28 October 2026

The ribbon is modestly shorter, with Capture retained at Home and Pen/Magnify moved to More tools. A far-right Styles button and panel dropdown replace the 40-point inspector rail. Styles is the default on startup, document changes and tool selection; Layers, Presets and Quick Effects retain their existing controls. The 260-point inspector, pin/unpin behavior, resizing and remembered tool styles/colors remain. Pin/unpin now uses a contextual pushpin icon. The one-time toolbar migration preserves other custom ordering and allows later re-pinning.

Quick styles now occupy **one row of four previews**, with **1 · 2 · 3** tabs beside the heading. Tabs 1 and 2 contain eight built-ins; tab 3 contains four persistent custom styles per tool. Set the desired properties and click an empty slot to save; click a saved slot to apply; right-click to update from current properties or clear. Saved styles include colors and applicable formatting, without storing document text, positions or arrow bends. Existing arrow geometry is preserved when applying a style.

**2.1.3 build 79** is installed at `/Applications/MyImage.app`, opened and confirmed running for user testing. The existing Developer ID identity/team and signing requirement are unchanged. Apple notarization `b03ec635-1eb3-4c14-b7e7-363c7f9a47dc` was **Accepted** using MyPDF; the ticket is stapled/validated and Gatekeeper reports **Notarized Developer ID**. Installed app, source bundle and ZIP match; the verified rollback is **2.1.3 build 78**. Settings were byte-identical before and immediately after installation.

Validation passed: **552 tests** (537 application + 15 packaging; 23 explicitly ignored), formatting, strict all-target workspace Clippy and whitespace checks. Native demo review checked ribbon/dropdown/overflow, all panels, pinning, the three style tabs, custom-slot saving/application and the minimum window size. Final screenshots and release evidence are in `dist/local-mac-v2.1.3-build79/`, including `native-review/`, source patch, logs, notarization receipt and signed/notarized ZIP. Source remains uncommitted on base `167f6a6a92d97497951dbe3cbc1085b2d652e42f`. User testing and Windows native interactive review remain pending; no GitHub release or Store change was made. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 78 — 27 October 2026

Following user feedback on build 77, the expanded inspector now defaults to **260 points wide**, down from 280 (about 7% narrower), in both floating and pinned modes. Panel padding is 8 instead of 10 points. Color swatch gaps adapt to the available width so all eight colors and the custom picker fit without shrinking their click targets. Pinning, divider resizing/reset, remembered panel state, existing controls and style/color preferences remain intact.

**2.1.3 build 78** is installed at `/Applications/MyImage.app`, opened and confirmed running for user testing. Developer ID identity/team and signing requirement are unchanged. Notarization `b4aba39d-cff3-432a-8148-aa31703a48a3` was **Accepted** with the existing MyPDF Keychain profile; the ticket is stapled/validated and Gatekeeper reports **Notarized Developer ID**. Installed app, source bundle and ZIP match. The verified rollback is now **2.1.3 build 77**. Settings were byte-identical before and immediately after installation.

All **547 tests** (532 application + 15 packaging; 23 ignored), formatting and strict Clippy pass. Native demo review checked all four narrower panels and pinning; screenshots are in `target/review/inspector-compact-width/`. Evidence and the signed/notarized ZIP are in `dist/local-mac-v2.1.3-build78/`. Source is still uncommitted on base `167f6a6a92d97497951dbe3cbc1085b2d652e42f`. User testing is pending; no GitHub release or Store change was made. Preserve the unrelated branding PNG deletion.

## Previous local Mac test build 77 — 27 October 2026

The requested compact inspector strip is implemented and installed locally as **2.1.3 build 77** at `/Applications/MyImage.app`, which was opened and confirmed running. Styles/Layers/Presets/Effects open from a 40-point rail, with an overlay by default and a resizable dock when pinned. The selected section, expanded state and pin preference persist across document changes/restart; existing tool styles/colors and all panel controls are preserved. Formatting, strict all-target workspace Clippy and **547 tests** passed (532 app + 15 packaging; 23 ignored). Native Mac demo review and screenshots are recorded in `docs/RELEASE-VALIDATION.md`.

Build 77 uses the existing Developer ID identity/team `HXDYTBC6ZT` and unchanged designated signing requirement. Apple accepted notarization submission `4ab3fe43-9ea1-4f98-98d4-3c34bc0713f8` using the existing **MyPDF** Keychain profile. The ticket is stapled and validated; Gatekeeper reports **Notarized Developer ID**. Installed app, source bundle and notarized ZIP contents match. The installer verified and retained **2.1.2 build 76** in `backups/MyImage-rollback-macos-aarch64.zip`; settings were byte-identical before and immediately after installation. Both prior app copies closed normally before replacement.

This is a local installation for user testing, with source changes uncommitted on base `167f6a6a92d97497951dbe3cbc1085b2d652e42f`. Evidence, source patch, signed/notarized ZIP, check logs and `macos-validation.json` are in `dist/local-mac-v2.1.3-build77/`. User testing is pending. No GitHub release or Store submission was changed; public build 76 and Store package 2.1.4.0 remain the snapshots below. The unrelated branding PNG deletion remains untouched.

<!-- CURRENT_RELEASE_START -->
## Public release snapshot — 27 October 2026

Project: `/Users/gokcol/SWPROJECTS/Utility/MyImage`, branch `main`. The old workspace path is stale; pass the new directory explicitly to shell commands.

| Item | Latest verified state |
| --- | --- |
| Released source | `b1e07218b2f14cf388c4ac28754a0b047bfe2514` |
| Published app | **2.1.2 build 76**; local Mac test installation is **2.1.3 build 80** above |
| GitHub | Public and latest, [v2.1.2-build76](https://github.com/gokcol/MyImage/releases/tag/v2.1.2-build76), published 26 October 2026 at 20:23 Istanbul |
| Mac | Published build 76 app, ZIP and DMG were verified; `/Applications/MyImage.app` now runs local test build 80 above |
| Mac signing | Published build 76 app/DMG and installed build 80 app are Developer ID signed, notarized, stapled and Gatekeeper accepted; current rollback is build 79 |
| Windows | x64 Setup and portable ZIP published; unsigned; installer lifecycle passed |
| Store package | **2.1.4.0** uploaded, saved and **Validated**; 2.1.3.0 removed |
| Store submission | **In certification / Pre-processing**, confirmed 27 October 2026; automatic publishing after certification passes |
| Remaining work | Local build 80 awaits user testing. No public build 80 release requested. Microsoft approval, signing and Store availability remain external and unconfirmed since the recorded snapshot. |

The user reproduced main-canvas flicker by capturing a window, adding an arrow, and both dragging/reshaping it and changing color/width/style. Contained gestures now retain the original screenshot texture; canonical autosave preserves workspace brightness while input remains blocked. Real canvas expansion retains its fixed sampling frame until release. Build 76 also includes build 75's **Ctrl+Shift+C** capture shortcut on macOS/Windows and remembered per-tool styles/colors across reselection, images and restarts. Existing capture shortcuts remain.

Formatting, strict Clippy and **542 tests** passed (**527 application + 15 packaging; 23 ignored**). Persistent-frame regressions cover arrow draw/move/resize/bend/style, boundary crossing, saving appearance and blocked/resumed edits. Independent code and artifact reviews passed. All four exact-source workflows passed: native macOS `40365679103`, Windows installer `40366073789`, real Store `40366078963`, test MSIX lifecycle `40365679085`. Interactive Windows capture/clipboard/device validation remains pending.

Mac notarization IDs: app `9a0c4f67-c62e-44c7-91fa-f370064a6178`, DMG `ee9a8141-47a9-47d0-be7b-0a95e04f8e68`, both Accepted. The existing **MyPDF** Keychain profile initially returned a transient lookup failure, then authenticated and completed both submissions without credential changes. Reuse it; no replacement credentials were created.

Current evidence:

- `dist/release-v2.1.2-build76/release-validation.json`, `macos-validation.json`, release notes, signing/notarization/install/check logs.
- `dist/windows-v2.1.2-build76/windows-validation.json` and artifacts.
- `dist/msix-v2.1.2-build76/artifact-validation.json` and **MyImage-2.1.4.0-store-x64.msix** (SHA-256 `3634301a5fd4a7a8573e6931dd40479054eda77b12717dcc5087a87060d26619`).
- Store text in `dist/release-v2.1.2-build76/store-whats-new.txt` and `store-reviewer-notes.txt` was saved and verified exactly after reloading. Existing relevant reviewer instructions are preserved. Evidence: `store-listing-saved.txt`, `store-reviewer-saved.txt`, `store-options.txt`, `store-draft-saved.txt`, and `store-submission.json`.

Store update and final submission are complete. Product **9PL3HWB23J38**, submission **1152921505702179656**, identity `OrhanGkl.MyImageOG`, publisher `CN=A9F0208B-CC43-472D-AE82-9FE4556DC935`. Preserve pricing/markets/ratings/capabilities/startup and publish-after-approval preferences. Firefox native UI automation cannot operate while the Mac screen is locked; do not alter security settings. If the upload link cannot open the chooser, the user previously opened it manually, after which exact-path selection worked. Do not claim Microsoft signing or availability before certification. The published `release-validation.json` records the verified certification state; do not resubmit or cancel this submission merely to repeat the completed release.

On 26 October, build 76 was uploaded/saved, replacing 2.1.3.0. The overview showed only 2.1.4.0 as Validated. Both notes were saved and verified after reload; automatic publish-after-certification was confirmed selected. Submission was activated, and on 27 October the live overview confirmed **In certification / Pre-processing** and automatic publishing after approval. Evidence is retained in `store-after-submit.txt` and `store-submission.json`. No required verification remains. The guarded native helpers in `/private/tmp/myimage-partnercenter-*.applescript` now discover the page container in either Firefox window layout. Reviewer notes save via the **Save description** link; the listing uses **Save**. A separate Firefox window holds MyImage’s overview.

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
- In local build 79, Pen and Magnify are in More tools by default; explicit later toolbar customization is retained. Overflow stays a compact chevron. Copy shortcuts and context actions remain.
- Text and callouts edit **in place** with aligned compact controls, rather than a large modal editor. There are twelve callout styles and a Magnify tool with twenty visual presets/linked source areas (in More tools by default in local build 79).
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
