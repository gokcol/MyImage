# Release validation — MyImage

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
