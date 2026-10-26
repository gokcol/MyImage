# Release validation — MyImage

## Version 2.1 — build 74

macOS capture now resolves equivalent mirrored-display IDs automatically between CoreGraphics and ScreenCaptureKit. No monitor-name selection is required. Every capture refreshes display IDs and geometry after its timer delay, so the overlay uses current bounds after mirroring, docking or resolution changes. Windows still requires the exact selected monitor; disconnected unrelated displays never fall back to another desktop. App version remains 2.1.0, Mac build advances to 74, and MSIX advances to 2.1.2.0.

Validation: 513 application tests and 15 packaging tests passed, with formatting, whitespace checks and strict workspace/all-targets Clippy. Mirror regressions cover exact selection, forward/reverse/sibling aliases, refusal of unrelated or zero IDs, and fresh geometry. A native metadata-only check on the connected mirrored setup resolved internal display 1 to the current external desktop 2 at 2560 × 1440, without capturing pixels. Release signing, installer and end-to-end capture results are recorded with the release artifacts.

## Version 2.1 — build 73

Canvas-resize autosaves no longer clear and reload the whole Recents thumbnail strip. Saves retain existing previews while replacing only the edited project's thumbnail, reject outdated asynchronous replies, and preserve Library selection and scroll during metadata updates. Recents retains its normal appearance while a project save blocks input. Public app version remains 2.1.0; Mac build is 73 and MSIX advances to 2.1.1.0 for ordered Store upgrades.

Validation: 508 application tests and 15 packaging tests passed, including a real resize/autosave regression with warm Recents textures, targeted thumbnail replacement, consecutive saves, and stale-reply rejection. Formatting, whitespace checks and strict workspace/all-targets Clippy passed. Platform release and signing results are recorded with the release artifacts.

## Version 2.1 — build 72

Extract text now works on the whole visible image when there is no pixel selection, and on the selected region when there is one. The editable review identifies its scope and only copies when requested. This release also includes the first-use recognition fix, cancellation and timeout handling, Capture Now’s direct screen-to-text clipboard mode, and the PDF scrolling-capture fix. Public version is 2.1.0 and the Store package version is 2.1.0.0; the existing Store identity is retained.

Validation: the native macOS OCR regression recognized both the selected region and the full image, including text outside that region. The eight text-extraction tests, formatting, whitespace checks and strict workspace/all-targets Clippy passed. The prior native check verified first-use extraction in 714 ms. Release signing, installer and Windows CI results are recorded with the release artifacts.

## Version 2.0 — build 68

The floating arrow and shape properties bars now show only essential icon controls: color, stroke weight, line pattern, and start/end caps or fill/outline. Pen selections retain color and weight. Weight uses a slim vertical slider with an accessible value. Bars use their measured size; popups choose available space around the selected drawing and stay still while weight changes its bounds. Advanced properties remain in the right inspector. App version stays 2.0.0; Mac build advances to 68 and MSIX to 2.0.1.0 for an ordered update to the published build 67.

Validation: 484 app tests and 15 packaging tests passed, with 16 explicitly ignored diagnostics/integration tests. Formatting, whitespace checks and strict workspace/all-targets Clippy passed. Regression coverage exercises arrow endpoints and line patterns, shape fill/color changes, preserved selection and Undo, and popup placement near each canvas edge. Isolated native macOS review confirmed the compact arrow/shape bars, vertical slider appearance and changes, shape fill, and native Undo restoring stroke weight. Windows interactive testing remains pending; release CI and signing results are recorded with the release artifacts.

## Version 2.0 — build 67

Version 2.0 packages the build 62–65 improvements below, including Pearl Compact Capture Now, compact Settings, opt-in start at login, native background/reopen handling, the complete Calico pointer family, the resizable inspector, and compact toolbar with Pen after Shapes. Public app version is 2.0.0 and the Store package version is 2.0.0.0, retaining MyImage's existing Partner Center identity and increasing from the previous 1.13.0.0 package. The Store startup task remains disabled by default. The existing portable document format is retained. A one-time library conversion matches legacy images to projects by embedded source pixels, creates verified projects for unpaired images, remaps Recents/favorites, and retires verified dated captures to Trash only after associations are persisted. External imports remain untouched. The editor now automatically creates and saves its canonical project, flushes before navigation/quit, and excludes flattened exports from Library and Recents. Deleting projects includes embedded pixels and remaining managed image companions.

Validation: 482 app tests and 15 packaging tests passed on macOS, including the native signing-requirement test. Formatting and strict Clippy passed. New coverage checks capture alpha, no-clobber project allocation, legacy source matching with renamed edited projects, idempotence and deletion tombstones, failed conversion/source preservation, automatic import/save/reopen, undo flush on navigation, quit flush, and exports staying out of Recents. A further targeted regression passed for recovered documents receiving a fresh autosave attempt after a previous save failure. An isolated native review created a blank canvas without a save dialog, autosaved a rotation to the same project, displayed a projects-only Library, and deleted that disposable project through Library’s Trash action. The regular installed app was not modified during this review.

## Pearl Compact Capture Now — build 65 (local update)

Capture Now now uses the selected Pearl Compact design with the user's refinements: lighter graphite surfaces, a rounded shell with a top pointer, a shared recessed five-mode selector, options grouped on the left, and a 108-point red circular Capture button on the right. Its native panel is 560 × 292 logical points, reduced from 560 × 300. The dark palette, including display dropdowns, is local to the launcher, independent of the editor's GUI finish. The viewport uses transparency around its rounded shell; the other app windows continue painting opaque panels. Defaults, capture destinations, immediate-release behavior, delay handling, tray/hotkey entry points, footer navigation and dismissal behavior are preserved. A capture shortcut is visible below the button; both available Mac shortcuts are described in its tooltip. This build also includes the inspector and Calico changes documented under build 64.

Validation: 477 app tests passed (15 explicitly ignored diagnostics/integration tests); strict app/all-targets Clippy, formatting and whitespace checks passed. The two launcher tests passed again after the final circular-button layout changes. The launcher input test verifies visible controls and bounds, settings/close icon actions, disabled capture protection, destination, pointer, immediate release and timer updates. Native macOS review verified the final red circle, lighter background, dark display dropdown, selected states and Escape dismissal; earlier checks covered clipboard/pointer/delay changes. The isolated review was closed. Windows interactive validation was not performed.

Build 65 was signed with the existing Developer ID identity, notarized (Apple submission `769fed45-6da3-4590-a10b-58eb56ae10b6`, Accepted), stapled and accepted by Gatekeeper. Installed after the user confirmed saving and quitting build 63; the installer verified the signer, retained the previous app for rollback, and preserved captures, projects, settings and permission records. Reopened the installed app and visually verified the final Capture Now panel at the user's existing UI scale.

## Resizable inspector and complete Calico pointer family — build 64 (local update)

The right inspector has a slim draggable left divider. Its default is the compact width required by its tabs and controls. Dragging widens it; double-clicking the divider resets the fitted width. Arrow keys resize a focused divider (Shift increases the step), and Home resets it. The width is shared by Styles, Layers, Presets and Effects for the session. Window resizing constrains it to leave room for the canvas without forgetting the user's chosen width. Restarting returns to the compact fit.

Calico now supplies equivalents for all 34 visible native cursor types, including crosshair, text, links, status, move, resize directions and zoom. Precision glyphs use high-contrast outlines, a warm center stroke and a small cat accent away from the hotspot. Fill retains its bucket, selected color and exact seed with a Calico badge. Hidden cursors remain hidden unless a custom tool explicitly requests its accent for the current frame. Editor, Settings, Capture Now and screen-selection overlays share the preference; Standard remains available. Artwork stays outside the document/export pipeline.

Validation: 477 app tests passed (15 explicitly ignored diagnostics/integration tests). Divider regression coverage includes drag and coalesced press/release input, double-click reset, window constraints, keyboard steps/focus and unchanged document state. All 34 visible Calico cursor types were rasterized and checked for valid hotspots; focus loss and the frame-limited custom-tool badge are covered. Strict app/all-targets Clippy, formatting and whitespace checks passed. Native macOS review confirmed the fitted Styles/Layers panel, keyboard widening and the Calico crosshair. Automated native dragging did not produce a visible resize; drag behavior is covered by editor input tests. The disposable review app was closed without changing its saved fixture. Windows interactive validation was not performed.

Build 64 was signed, notarized (Apple submission `f5df00bd-283a-4a68-a204-4facdb11d3c4`, Accepted), stapled and accepted by Gatekeeper. It was not installed before the user selected Pearl Compact; build 65 supersedes it with that launcher design.

## Compact Settings and Calico pointers — build 63 (local update)

Settings uses two compact rows of preview tiles for the three GUI finishes and three icon styles, followed by a Standard/Calico pointer selector and the opt-in login checkbox. The manual startup refresh button is removed. OS startup registration is queried when Settings opens, when the app regains focus with Settings open, and after changing the checkbox; ordinary repaints do not poll it. Login startup remains off by default.

The opt-in Calico pointer uses small static cursor artwork: a cat-accented arrow, an open paw over movable objects and a closed paw while dragging. Pointer hotspots and object hit testing are unchanged. Text, resize, busy, capture and precision-tool cursors are preserved. The input-transparent artwork is rendered only in the app UI, outside capture mode, and never in document exports. Standard remains the default for existing and new settings.

The top annotation toolbar removes its redundant Copy group and uses a compact borderless chevron for the overflow menu. Keyboard/context-menu copying and the menu’s tools/customization remain available; pinned tools retain their order.

Pen is pinned directly after Shapes. A one-time layout migration applies this to existing preferences while preserving the other tools’ order; later customization remains persistent. The existing native pen/touch input path supports fixed-width freehand strokes, including finger contact. Pressure-varying stroke width and palm rejection are not added. Native pen/touch hardware was unavailable; paired touch/pointer event tests cover stroke creation, lift-off and one-step Undo.

Validation: 475 app tests passed (15 explicitly ignored diagnostics/integration tests), including pointer fallback/focus, saved preferences, the one-time toolbar migration, startup default/manifest and paired pen/touch input. Strict app/all-targets Clippy, formatting and whitespace checks passed. Native macOS review verified compact Settings at Very Large UI size, complete labels, Calico selection and the open paw over an arrow, the borderless overflow menu and tool selection, Pen placement, drawing and Undo. The final disposable pointer review was closed after Undo restored the saved fixture. The earlier compact Settings review received manual edits and was left open to preserve that work. Pen/touch hardware and Windows interactive validation were not performed.

Build 63 was signed, notarized (Apple submission `5c29f032-7b8e-4611-a8b9-d044d5df0663`, Accepted), stapled and installed with the normal verified rollback. The installed app was opened successfully. The user closed the earlier Settings review before installation.

## Background recovery and login startup — build 62 (local update)

Dock/Finder reopen requests now enter the existing Show handler through an optional AppKit delegate method. The callback is queued and wakes the root viewport; native quit protection is preserved. The resident runtime retains the editor window identity and its occlusion state, rather than replacing them with Capture Now child-window events.

Settings has an opt-in **Start MyImage at login** control, off by default. It reads OS registration state instead of persisting an optimistic boolean. macOS uses SMAppService.mainAppService; packaged Windows uses a manifest startup task with Enabled=false; unpackaged Windows uses the current-user Run entry. System disable/policy states are respected. No startup registration is performed automatically during initialization, settings migration or installation.

Validation: 469 app tests passed, followed by the new root-window regression test and startup default/manifest checks. Strict Clippy passed on macOS and for all Windows targets via cross-compilation. Native macOS review verified Background → reopen, native quit, and reopening again after opening/dismissing Capture Now. The signed disposable review app successfully registered and unregistered login startup; refreshing confirmed Off afterward. Real restart/login and Windows interactive testing were not performed.

Build 62 was signed with the existing Developer ID identity, notarized (Apple submission `6e7b2410-9dbb-4efc-8c38-0dede81bd70e`, Accepted), stapled and accepted by Gatekeeper. After the user confirmed saving and quitting, the normal installer retained rollback and installed version 1.3.0 build 62. The installed app opened successfully; captures, projects, settings and permission records were preserved. The user resumed interaction in Library, so further UI actions stopped. No GitHub release or Store submission was changed.

## Version 1.3.0 — build 61

This release combines the inline text/callout editor, twelve callout shapes, visual Magnify, compact Library and toolbar refinements, dedicated Fill pointer, three GUI finishes and three independent icon styles. Capture Now retains its 560 × 300 layout with refreshed controls. Project format remains 22. The Store package advances to 1.13.0.0 with the existing Partner Center identity. Windows distribution remains through Microsoft Store; direct Windows downloads are manual-only and still require a trusted production signature.

Local validation passed: 467 application tests and 15 packaging tests, with 16 explicitly ignored diagnostics/integration tests; formatting, strict workspace Clippy and whitespace checks passed. The packaging signer test passed with normal macOS trust access after the sandbox rejected Apple system-binary verification. Apple signing/notarization and GitHub CI results are recorded in the v1.3 release notes. Windows interactive validation of this version remains pending. Publishing GitHub artifacts does not install the update or submit it for Microsoft certification.

## Version 1.2.0 — build 59: inline editing and visual Magnify

Text and callouts edit directly on the canvas with a compact formatting strip and shared rendering/alignment metrics. Callouts offer twelve visual shapes. Magnify adds twenty visual presets, oval/rectangular/rounded lenses, linked source regions, adjustable zoom and outlines, and a draggable source point. Magnify is included in the default main toolbar; More retains its label and uses a narrower menu. Toolbar customization fits its contents, and the Library sidebar has denser rows, calendar icons and clearer selection styling. Project format is version 22.

Validation: 462 application tests passed, with 15 explicitly ignored diagnostics/integration tests. Strict Clippy, formatting and whitespace checks passed. Native review exercised the Magnify gallery, linked source dragging, lens resizing, and toolbar controls. The final narrower menu/customization layout and Library refinements compiled successfully, but their last native visual check was interrupted by UI automation timeouts. Windows interactive validation of these changes remains pending.

The optimized macOS bundle was signed with the existing Developer ID identity and passed signature verification. After the user restored the existing `MyPDF` Keychain profile, Apple accepted notarization submission `35628b5f-5b66-4a4b-a31c-aa629fe0347c`; stapling, ticket validation and Gatekeeper assessment passed. Installed build 59 with a verified rollback of the previous app, preserving captures, projects, settings and permission records. Launched the installed app and opened its Editor successfully.

## Version 1.2.0 — build 58

Public version 1.2.0 consolidates builds 50–57: visual Selection/Layers/Presets, compact Recents and stable ordering, clipboard capture and recent-file Copy, expanded editable image effects, five graphical stroke patterns, multiple-region capture, optional capture on release and the compact native Capture Now launcher. Mac build numbers continue increasing. Project format is version 21. Store package version is independently 1.12.0.0 to preserve ordering after 1.11.x.

Microsoft Store support follows MyPDF’s packaging approach: exact Partner Center identity validation, DPI artwork, MakePri/MakeAppx semantic checks, executable/identity round-trip verification and disposable install/uninstall tests. The public privacy policy is linked in About. Store package identity, certification and production Windows signing require maintainer configuration; a test certificate is never a public release signature. Release publication requires a valid timestamped Windows signature before uploading a Windows download.

Release validation and artifact signing results are recorded in the GitHub v1.2 release notes. Windows interactive testing remains pending.

## Version 1.11.1 — build 57: compact resident capture panel

Left-clicking the menu-bar/tray icon opens a separate native Capture Now window. The panel shares capture settings with the main app, keeps all five capture modes, and provides destination, display, pointer, capture-on-release and delay controls plus Editor/Library/settings navigation. Right-click retains the native menu and hotkeys still capture directly. The final panel measures 560 × 300 logical points, approximately 26% less area than the initial 650 × 350 layout, with shorter tiles and a smaller round Capture button.

Validation: 453 application tests and 15 packaging tests passed (16 explicitly ignored diagnostics/integration tests). Strict Clippy passed. New tests exercise all shared settings, the Capture action, text bounds, and panel placement at display edges and a bottom tray. Native macOS visual review confirmed the 560 × 300 panel, smaller controls and fully visible footer. The UI automation tool refocused the main window when attempting child-window interactions, dismissing the panel; shared control updates are covered by the interaction test. Both isolated review apps were closed and removed. Signed with the existing Developer ID identity, notarized (submission `38273d72-f974-4f00-8263-759a82c4eeb2`), stapled and accepted by Gatekeeper. Installed build 57 after the user confirmed saving and quitting. Installation verified the signature and notarization, retained the previous installed app as rollback, and left captures, projects, settings and permission records unchanged. Reopened the installed app and visually verified the compact Capture Now panel at the user’s existing UI scale. Windows interactive validation remains deferred.

## Version 1.11.1 — build 56: capture timing and multiple regions

Capture options now include opt-in capture on mouse release; existing settings retain explicit confirmation. Region and Window modes support immediate completion. Multi-region capture always waits for Capture/Enter, combines up to 32 rectangles at their original relative positions, and zeroes RGBA outside the selected areas. Regions can be added, moved, resized and removed before capture. Command on Mac / Ctrl on Windows starts an additional region, including overlapping regions, without changing the saved capture mode.

Validation: 451 application tests and 15 packaging tests passed, with 16 explicitly ignored diagnostic/integration tests. Strict Clippy, formatting and whitespace checks passed. Tests cover legacy settings, capture completion gating, region adjustment/removal/limits, overlap alpha preservation, transparent gaps and invalid pixel bounds. Native macOS demo review verified two regions, moving a previous region, the transparent combined image, immediate capture on release, default confirmation, Command-click adding a region and Backspace removal. Snagit's helper could not be controlled reliably by automation; the user performed its multi-region experiment and confirmed one image with transparent gaps. Windows interactive validation remains deferred.

Signed with the existing Developer ID identity, notarized (submission `8283fbff-2e95-4d0a-a049-f896cfd5fc28`), stapled and accepted by Gatekeeper. Installed build 56 successfully captured two real screen regions into a transparent PNG without renewing Screen Recording permission. The fast-capture setting remained off. The isolated QA copy and its two generated captures were removed; automatic release cleanup retained the latest rollback.

## Version 1.11.1 — build 49

Editor now exposes eight compact canvas resize handles outside the image. Corner and edge drags change the visible canvas window while retaining the original pixels, annotations, crop, rotation, flip and cut-out state. Growing adds transparency; shrinking keeps hidden content editable and recoverable by later expansion, including after saving/reopening. The drag uses a fixed input frame and a reusable bounded composition texture, with live dimensions, resize cursors, Escape cancellation and one Undo step. Pixel-selection handles take priority; blank canvas presses do not swap preview textures. Fit and pointer-anchored pinch reserve room for the handles.

Recents and cached Library previews check missing paths on the existing background worker while visible, at most once per path per five seconds. Confirmed missing files are removed from browser entries, thumbnails, selection, favorites and persisted Recents. Permission errors and corrupt files remain available for diagnosis. Missing-file races during Open, Trash, Duplicate and Rename no longer produce repeated filesystem errors, and an in-memory document is preserved even when its source disappears.

Validation: 419 application tests and 15 packaging tests passed; 16 diagnostic/integration tests were explicitly ignored. Formatting, diff whitespace and strict workspace Clippy passed. Coverage includes all eight handles across drawing tools, stationary drag sampling, click-only and quick gestures, invalid sizes, cancellation, keyboard Undo focus, hidden-layer save/reopen under rotation/flip/nonuniform resize, pinch anchoring, background availability checks and missing-file operations. Native macOS review verified transparent growth, shrink/reveal, immediate Undo/Redo, project saving, and automatic removal of an externally deleted selected recent entry while its unsaved arrow and in-memory image remained intact. The isolated review app was closed and removed. Windows interactive validation remains deferred; final tagged-source CI, signatures and notarization are recorded in the GitHub release notes.

## Version 1.11.0 — build 48 (release tag v1.11)

This release combines build 46's editable drawing beyond the canvas with the Studio Contrast dialogs reviewed in build 47. The two-part public tag `v1.11` maps exactly to package version `1.11.0`; the Windows workflow validates that normalization before uploading its explicitly UNTESTED package. Project format 18 and the stable macOS bundle/signing identity are unchanged.

The feature source passed 412 application and 15 packaging tests, strict workspace Clippy, formatting and the native checks recorded below before the version-only release bump. Final tagged-source CI, signing, notarization and package verification are recorded in the GitHub release notes.

## Version 1.1.0 — build 47: Studio Contrast dialogs

All application-owned dialogs and prompts use shared native Rust/egui surfaces: charcoal headers, contextual icons, neutral bodies, compact raised action buttons, distinct destructive actions and soft shadows. This covers export, image/canvas sizing, text, SVG stamps, preset/layer naming, toolbar customization, library actions, storage migration, crash recovery, the first-run guide and unsaved changes. Error notices and clipboard confirmation use matching styling. macOS/Windows file and folder pickers remain platform-native.

Validation: 412 application tests and 15 packaging tests passed, with 16 explicitly ignored diagnostic/integration tests. Formatting, diff whitespace validation and strict workspace Clippy passed. The new bounded-prompt regression covers three workspace sizes and verifies Escape does not activate an action. Existing native-control tests still exercise layer rename, preset save/rename/delete and text-editor sizing.

Native macOS review verified the welcome guide, export, canvas sizing, text entry/apply, unsaved-work cancellation and discard on a disposable fixture, and the storage dialog at Very Large UI text size. Final checks confirmed header close-button positioning and action icon/label spacing. The authorized isolated review app was closed and removed; regular user artwork was untouched. Windows interactive validation remains deferred.

## Version 1.1.0 — build 45

This release consolidates builds 41–44: layer-aware selection deletion, the refreshed Calico guide, natural object selection in drawing tools, directly editable curved arrows, compact eight-point shape handles, stable canvas layout during selection, and improved window detection and capture controls. New saves use editable project format 18; older projects remain readable.

The Windows workflow reads the package version from Cargo metadata, uses it in the archive and UNTESTED notice, and verifies it matches the release tag before attaching assets. Windows interactive validation remains deferred.

Release-candidate validation on Rust 1.93.1: all 404 application and 15 packaging tests passed; 16 explicit diagnostic/integration tests remain ignored. Formatting and strict workspace Clippy passed. The native interaction checks for the unchanged feature code are recorded under builds 41–44 below. Final notarization submissions, packaged asset checks and GitHub build results are recorded in the v1.1.0 release notes.

## Version 1.0.0 — build 40

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

## Build 41 — selection deletion

The canvas context menu offers **Delete selected area** for a pixel marquee and **Delete selected layers** for annotation/group selections. Pixel deletion is an editable removal layer, respects stack order and preserves original pixels. Delete and successful Cut clear the marquee, masks and object selection. Choosing a layer also clears a previous pixel marquee. Locked selections cannot be deleted.

The 393 application tests and 15 packaging tests pass, including transparent pixel deletion, single-step undo/redo, editable save/reopen, layers above removal, group/lock handling, lasso and wand masks, and clipboard failure safety. The codesign integration test requires access to the macOS trust service outside a restrictive sandbox.

Native macOS verification used an isolated demo bundle and a generated two-color fixture: right-clicking a pixel marquee exposed Delete selected area, deletion displayed transparency with no lingering handles, and Undo restored the image. The regular app and its unsaved project were left untouched.

The refreshed first-run guide uses the existing calico artwork, three illustrated action cards, a recovery tip and an expandable shortcut reference. It remains available from the App menu; the guide version was advanced so existing users see the refreshed guide once. The body scrolls independently of the dismissal button.

The final guide was checked natively at Normal and Very Large UI text sizes, including expanded shortcuts and scrolling. Shortcut columns remain readable and the dismissal button remains visible. Formatting and strict workspace Clippy passed. The isolated review app was closed and removed after testing.

Final build 41 notarization submission `fb96331c-5d0a-414e-9185-a927d8ead3fb` was accepted, stapled and validated; Gatekeeper reports Notarized Developer ID. The retained rollback is build 40. Build 41 was subsequently installed after the user saved and quit; the installed build 40 was retained as rollback.

Post-install capture verification: macOS TCC diagnostics identified a stale ScreenCapture requirement tied to the old local-development certificate. Resetting only `com.myimage.app` ScreenCapture and renewing its grant resolved the mismatch. The installed Developer ID build 41 then opened a real capture overlay and saved a 1603 × 613 PNG into the configured Pictures/MyImage library, displayed in Editor with Capture saved status. No other application permission records were reset.

## Build 42 — object editing and capture selection

Drawing tools now select and drag existing unlocked annotations before creating a new object. Shift supports multi-selection; Alt/Option forces drawing over existing artwork. The active drawing tool stays available for the next empty-space gesture. Existing text and stamps are no longer accidentally inserted when clicking another object.

Arrows and lines support editable cubic curves: drag the center handle to bend, then refine with control diamonds or move either endpoint. Straighten is available in the properties panel and floating style controls. Hit testing follows the visible curve. Control points survive movement, grouping, proportional group resize, duplication, clipboard transfer, exports and project reopening. Project format 18 stores these points; older straight-arrow projects remain readable.

Capture Now highlights candidate windows in Region mode: click a window or drag a free region, then adjust the bounds. Its controller offers Region, Window and Scrolling modes and moves near the completed selection. Space switches Region/Window; Tab cycles overlapping candidate windows. The native window backend continues to supply front-to-back candidates.

Native macOS review confirmed Snagit's center-to-bend arrow behavior on a disposable image. Its Capture command redirected to a hidden menu-bar notice, so a complete native Snagit capture comparison was unavailable. MyImage's isolated demo review verified arrow drawing, bending, moving, editable save/reopen, window click selection, boundary adjustment, Space switching and cancellation. The review app was closed after saving its disposable project.

Final checks: 398 application tests and 15 packaging tests passed; 16 diagnostic/integration tests remain explicitly ignored. Formatting and strict workspace Clippy passed. Curve coverage includes transparent raster rendering, tangent-aligned endpoints, version validation, group resize, clipboard round trips and all export formats. Windows interactive validation remains deferred.

Build 42 notarization submission `979e4621-dc4c-4201-afb9-fbf3d965d636` was Accepted, stapled and validated; Gatekeeper reports Notarized Developer ID. The installer replaced the cleanly closed build 41 and verified its rollback. The regular app was reopened. Real window highlighting appeared in the capture overlay; the final capture was saved in Region mode as a 2389 × 1319 PNG and displayed in Editor. The existing ScreenCapture grant worked without a reset or renewal. Build cleanup left approximately 40 MiB in target and 12 MiB in the retained rollback directory.

## Build 43 — direct curve adjustment

The two yellow curve handles now sit on the arrow at one-quarter and three-quarters along its cubic path. Dragging either point solves the internal control points so the grabbed location follows the pointer while the opposite yellow point and both endpoints remain fixed. The initial center bend handle, midpoint and endpoint controls remain available. External control-arm lines are no longer shown. Existing format-18 projects gain the new interaction without a file-format change.

Regression coverage exercises both handles through multiple drag updates, a single undo/redo step, save/reopen and hit detection after rotation, image resizing and zoom.

All 400 application tests and 15 packaging tests passed. Both direct-handle regression tests were rerun after a test-only lint correction, and strict workspace Clippy and formatting passed. Native macOS review opened the build-42 curved-arrow project, selected the arrow in Arrow mode, and dragged both yellow points independently. The points landed at the requested screen positions while the opposite point and endpoints stayed fixed, with no external control lines.

Build 43 notarization submission `86cae568-0910-40bb-89e0-a06736e242dd` was Accepted, stapled and validated; Gatekeeper reports Notarized Developer ID. The installer verified and retained the installed build 42 rollback, installed build 43, and left captures, projects, settings and permission records unchanged. The isolated review app was closed and removed. The installed app was reopened in Editor on the previously open image. Automatic release cleanup removed build caches and retained the latest rollback.


## Build 44 — stable selection and compact object handles

Drag-created arrows, shapes, highlights, pen strokes and effects stay unselected while drawing and after release. Clicking the finished object shows its handles. Callout creation still opens its text-entry workflow. The inspector now reserves its footer's actual height before laying out scrollable properties, fixing the workspace growth that could refit the canvas and accidentally move an object during a stationary click. Blank-image and canvas-margin clicks deselect objects; a margin click retains canvas keyboard focus. Handle presses retain their grab offset and do not change geometry on a click alone.

Arrow endpoints and closed-shape handles are smaller white circles with dark outlines; curve adjustments use small yellow diamonds on the path. Closed shapes gain four side-midpoint handles in addition to their corners. Side drags resize one axis while preserving the opposite side. Small handles retain an eight-point invisible hit radius, and overlapping targets choose the nearest handle. Step markers preserve uniform corner resizing. Shape selection does not draw a bounding line over the shape's own outline. Existing project format 18 stores the same editable geometry.

Validation: all 404 application tests and 15 packaging tests passed (16 explicitly ignored diagnostic/integration tests). Packaging signature validation was rerun outside the sandbox because macOS trust checks are unavailable inside it. The full 404-test application suite passed again after the final canvas-margin keyboard-focus correction; strict workspace Clippy and formatting passed. Regression coverage includes every-frame canvas bounds and zoom through select/hold/release/deselect, blank-margin deselection, unselected draft drawings, side-handle grab offsets, unchanged opposite axes, undo/redo, project reopening and overlapping hit targets after rotation, flip, resizing and zoom.

Native macOS review confirmed a saved curved arrow stays saved and at the same 84% zoom when selected and deselected, new arrows and ellipses finish without handles, and explicit selection shows them. The updated white/yellow handle styling and eight-point rounded-rectangle selection were reviewed. After the user authorized resuming the review, a right-side midpoint drag widened the rounded rectangle while its top, bottom and left edge remained fixed. Undo restored the disposable project to its saved state, and the review app was closed and removed. Windows interactive validation remains deferred.


Build 44 notarization submission `1d096dd8-3cfe-441a-87a9-46d616a49d5a` was Accepted, stapled and validated; Gatekeeper reports Notarized Developer ID. The regular app was confirmed closed before installation. The installer verified and retained the installed build 43 rollback, installed build 44, and left captures, projects, settings and permission records unchanged. The installed app was reopened at Home, and its App menu confirmed `1.0.0 · build 44`. Release cleanup left approximately 40 MiB in target and 13 MiB in backups.

## Build 46 — draw beyond the canvas

Object tools can start in the empty workspace and draw across any canvas edge. Drawing, moving, resizing, duplicating and nudging annotations or groups automatically grows the visible canvas with transparent margins. Text, stamps and numbered steps also support placement outside the image. Original pixels and editable layers are retained. Pixel-selection, crop and fill tools keep their image-based interaction boundaries.

During an object gesture the preview grows while input coordinates and its sampling frame remain fixed. Fit mode shows the growing canvas during the drag. The geometry change and object edit commit as one undo step; Escape or rejected oversized growth restores both. Existing curved arrows retain their shape in previews and exports when the canvas grows left or upward. Removing an empty text layer does not expand the canvas around an unrelated hidden object.

Validation: 411 application tests and 15 packaging tests passed, with 16 explicit diagnostics/integration tests ignored. The packaging signature test requires the macOS trust service outside the restricted sandbox. Formatting and strict workspace Clippy passed. Regression coverage includes all four edges, wholly outside drawing, live preview without coordinate drift, transparency, undo/redo, cancellation, editable project reopening, grouped movement and resizing after rotation/flip/nonuniform image resizing, text/stamp/step placement, and unchanged curved-arrow pixels after negative-origin expansion.

Native macOS review used a disposable 400 × 240 image in an isolated app. An arrow started entirely outside the image expanded the canvas left and upward to 487 × 327 with checkerboard transparency. Another arrow crossed the right and bottom edges and grew it to 532 × 292. Each Undo restored the clean 400 × 240 document. The review app was then closed. Windows interactive validation remains deferred.

Build 46 notarization submission `ab45e550-610d-460b-b1eb-9ea99708738f` was Accepted, stapled and validated; Gatekeeper reports Notarized Developer ID. Release cleanup removed build caches and retained the rollback. The isolated review bundle was removed. After the user saved and quit, the installer verified the signature and notarization, retained the actual installed build 45 as rollback, and installed build 46 without modifying captures, projects, settings or permission records. The app was reopened at Home and its App menu confirmed `1.1.0 · build 46`. The published v1.1.0 release and its Windows UNTESTED artifact are unchanged.


## Cobalt Satin GUI finishes — unreleased source update (2026-10-20)

Cobalt Satin is the default GUI finish. App menu → Settings · Appearance offers Cobalt Satin, Amber Enamel and Jade Graphite, applies each immediately, and persists the choice in existing settings. Legacy settings select Cobalt Satin without changing interface size or other preferences. Colors are context-local; annotation colors, image data and project serialization are independent of the interface finish.

Shared chrome, editor tools, capture controls, Library/Recents, properties, floating controls and dialogs use the selected palette. Selected controls use a subtle satin gradient, fine border and restrained shadow. Existing control positions, dimensions, typography and spacing are preserved; the new Appearance entry is appended to the existing app menu.

Validation: 462 existing application tests passed (15 explicitly ignored), plus three new tests covering settings compatibility/persistence, context isolation and unchanged control geometry/scaling, and text contrast. All four appearance tests and the new settings test passed. Strict workspace Clippy with all targets, formatting, debug build and diff whitespace checks passed. Native macOS review verified all three finishes switch immediately and inspected Cobalt Satin in the editor and Settings dialog. A screenshot is retained at `target/review/cobalt-satin-editor.png`. The review copy received manual edits and was left open to preserve its unsaved work. This source update has not been packaged, installed or published; Windows interactive review remains pending.


### Build 60 installation (2026-10-20)

After the user saved and quit both apps, the Cobalt Satin source update was packaged as version 1.2.0 build 60 with the existing Developer ID signing identity. Apple notarization submission `a966faf7-1506-43cc-8324-0cf96320e155` was Accepted; its ticket was stapled and validated, and Gatekeeper reported Notarized Developer ID. The installer verified and retained installed build 59 for rollback, installed build 60, and preserved captures, projects, settings and permission records. The installed app was opened successfully at Home for the user to try. No GitHub release was published by this installation.

## Unreleased — selectable icon styles and Capture Now polish

Appearance settings now offer independent Studio Color (default), Cobalt Duotone and Precision Outline icon styles alongside the three GUI finishes. Native vector glyphs replace the principal editor and shared action icons; restrained family colors, rounded strokes and subtle fills respond immediately to the selection. Selected, disabled and destructive foreground colors remain intact. Existing settings migrate to Studio Color; all nine finish/style combinations round-trip without altering capture preferences.

Capture Now retains its 560 × 300 layout, control positions and actions, with updated capture-mode icons, a satin capture button, quieter outlines, softer switches and a highlighted header edge. The editor and document rendering are unaffected by interface color preferences.

Validation: 465 existing application tests passed; two added tests passed for icon bounds and context/semantic-color isolation. Updated persistence and capture-control checks passed, together with strict workspace Clippy, formatting and whitespace validation. Native macOS review verified Appearance choices and the compact Capture Now panel; screenshot: target/review/studio-color-capture-now.png. Review copy left open after it received user input. This source update is not yet packaged, installed or released; the regular installed app remains build 60. Windows interactive verification remains deferred.


## Build 75 — 2.1.1 capture shortcut and remembered tool appearance (2026-10-26)

Control+Shift+C starts capture on macOS and Windows alongside the existing Control+Shift+period shortcut and the macOS Command+Shift+period alternative. macOS resolves C and period independently from the active startup keyboard layout. An unavailable character or conflicting registration leaves the other shortcuts working. Capture still fires once on release; retry preserves working registrations and held-key state. Compact launcher, capture settings, tray and quick-start hints show the new shortcut.

Drawing tools remember their explicitly chosen styles and colors independently, including arrow endpoints and line patterns, stroke width, shape fill, text/callout formatting, and pen/highlighter/focus appearance. These defaults survive tool reselection, document changes and app restart. Inspecting existing artwork does not replace drawing defaults; canceled text edits do not change them. Fill color/options and eraser size also persist. Saved preferences exclude document text, absolute arrow geometry, identities and step numbers. Pending preferences are flushed after editing and before quit/hide/document changes. Existing settings retain compatible defaults. Stamp artwork and its separate opacity controls retain their existing behavior.

Local validation of the final source: formatting, strict workspace Clippy and all-target workspace tests passed: **524 application tests and 15 packaging tests**, with **23 explicitly ignored** diagnostics/integration tests. New coverage verifies independent and relocated shortcut characters, partial registration and retry, per-tool properties/color, settings round trips, old settings, invalid stored values, selection isolation, new-arrow geometry, inline text commit/cancel/content-only edits, and fill persistence. Independent source reviews found no actionable correctness issues. Windows interactive capture/clipboard/device validation remains pending.

Release version is **2.1.1 build 75**; the separate Store package version is **2.1.3.0**. Signing, notarization, installation, exact-source Windows workflow results and publication/submission status are recorded in the release validation JSON attached to the release.

## Build 76 — 2.1.2 annotation canvas stability (2026-10-26)

Reproduction: capture a window, add an arrow, then move/reshape it or change its color, width or style. Two independent rendering transitions affected the main canvas. Contained object gestures unnecessarily reserved an expanded canvas, replacing the original screenshot texture with a reduced interactive composite until release. Canonical project autosave also disabled the workspace with the normal faded appearance, briefly changing the screenshot’s brightness after property edits.

Contained object edits now keep the original source texture. A retained sampling frame starts only when annotation bounds require canvas expansion and remains fixed for the rest of that gesture, including returning exactly to the starting position. Project saves retain the workspace’s normal appearance while the existing input lock stays active. Unrelated work and obscuring dialogs keep their prior disabled appearance. Privacy compositions and their stale-result protections are unchanged.

Persistent-frame regressions cover arrow drawing, moving, endpoint resizing, curve bending, and color/width/pattern edits; they compare the source texture and sampling coordinates through each frame. Boundary-crossing coverage verifies retained-frame lifetime. The save regression compares image meshes before/during/after saving, blocks pointer edits during the save, and verifies editing afterward. Independent reviews of both production changes found no actionable correctness issues.

Final local validation passed: formatting, strict workspace Clippy, and **527 application + 15 packaging tests (542 total)**; **23 diagnostics/integration tests were explicitly ignored**.

Release app version is **2.1.2 build 76**, with separate Store package version **2.1.4.0**. Final checks, signing, installation, workflow and publication/submission evidence are recorded in the build-76 release validation report.
