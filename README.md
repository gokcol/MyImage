# MyImage

A native Rust screen-capture application for **macOS 14 or later and Windows**, built with egui, winit, and wgpu. It does not use a WebView.

[Download MyImage 1.2](https://github.com/gokcol/MyImage/releases/tag/v1.2) — notarized macOS Apple Silicon. **Windows x64 remains UNTESTED**; its release download requires production signing.

MyImage opens in **Home**, with **Editor**, **Library**, **Capture**, and **Create** beside it in the compact top toolbar. **View only** remains available from the bottom **App** menu. The native editor has a drawing toolbar, a large canvas, and an annotation inspector. Add editable text, arrows, 17 shape types, stamps, numbered steps, callouts, highlights, and freehand pen strokes; select, move, resize, recolor, reorder, or delete them. Add adjustable blur and pixelate regions to obscure selected areas, or use Fill to recolor or clear connected backgrounds. Crop, resize, and rotate the image without changing its original pixels. Undo and redo cover both image edits and annotations during the current session.

Save **`.myimage` projects** to keep the original image and annotations editable together in one portable file. **Export…** creates flattened PNG, JPEG, WebP, TIFF, or PDF copies with visible annotations included. Image edits are stored as reversible settings in the project.

Real capture, rounded PNG saving/viewing, and both physical capture shortcuts were verified on macOS in the capture milestone. Annotation and library milestones include automated tests and native macOS browsing, save/reopen, and editing checks. The export, image-edit, privacy, markup, freehand, grouping, layout, layers, clipboard, and 0.13.0 preset milestones add independent image/PDF checks and native macOS editing, export, and project-reopening checks. Results are recorded in [the validation record](docs/RELEASE-VALIDATION.md). Windows runtime validation remains pending.

## Microsoft Store and signing

The macOS release is signed with Developer ID and notarized. Windows builds remain **UNTESTED**. A GitHub release download is uploaded only
after its production signature and timestamp are verified; signing configuration
is still required. Store packaging is a separate MSIX
pipeline modeled on MyPDF, with identity validation, installation checks and
calico assets. Microsoft signs Store packages after certification; an unsigned
submission artifact is not a signed end-user installer. See the
[Store preparation guide](packaging/windows/msix/README.md) and
[privacy policy](PRIVACY.md). MyImage 1.2 uses internal MSIX version 1.12.0.0 to
preserve upgrade ordering after earlier public 1.11.x releases.

## Run

Install rustup (the repository pins Rust 1.93.1) and your platform's native build tools: Xcode Command Line Tools on macOS, or the Visual Studio C++ build tools with a Windows SDK on Windows.

```sh
cargo run
cargo run -- --background
```

`--background` starts with the window hidden after the menu-bar/system-tray icon is available. It does not install a login item or startup service.

For a local macOS app bundle:

```sh
cargo run -p xtask -- bundle
cargo run -p xtask -- bundle --debug # isolated output in target/bundle/debug/
open target/bundle/MyImage.app
```

Bundles use optimized release builds by default; `--release` is also accepted explicitly. `--debug` keeps development builds available. The bundle has a stable location at `target/bundle/MyImage.app` and bundle identifier `com.myimage.app`. macOS packaging requires a persistent signing certificate, verifies the signature, then creates a ZIP alongside the app. Configure the certificate once using [the signing setup guide](packaging/macos/SIGNING.md). Builds never silently fall back to ad-hoc signing; disposable development builds must explicitly pass `--ad-hoc`. Distribution signing and notarization remain separate release work. On Windows, the same helper produces `target/bundle/MyImage.exe`.

Install the macOS release at `/Applications/MyImage.app`. Open that copy for everyday use. Settings, captures, projects and annotation assets remain in their existing folders.

Home is the initial workspace and sits immediately before Editor. It provides the calico Open image, Capture, Paste image and Create chips plus Recents. Returning Home keeps the current document and its edits; Continue editing returns to that image. Opening another image still uses the normal unsaved-changes protection.

The bottom-right calico App menu includes **UI text size**: Small (90%), Normal (100%), Large (115%) and Very Large (130%). The choice applies immediately and persists across launches. Interface labels and controls scale together; image pixels, exported dimensions and annotation font properties do not change. Canvas pinch zoom remains independent.

The calico screen-capture cat is bundled as the macOS Applications/Finder icon and the running app's Dock/window icon. The original transparent artwork and generated icon assets are in `assets/branding`; `cargo run -p xtask -- icons` regenerates the assets on macOS.

Recents, Library and Open share a background image loader. It decodes and prepares GPU-ready pixels off the UI thread, including the initial composition of editable effects. A 128 MiB / four-file CPU cache reuses recently opened originals and display pixels, checks file metadata before reuse, and releases its entries when the app is hidden to the menu bar/tray. A separate 64 MiB / three-image GPU texture cache avoids re-uploading the same version-checked source on warm opens; it is also cleared when the app is hidden. Original resolution and editable annotations are retained. Thumbnail uploads are spread across frames to keep browsing responsive.

## Autosave and crash recovery

Unfinished images and projects are automatically saved to a **separate editable recovery copy**, about five seconds after changes, once a canvas gesture is complete. Encoding and disk writes run in the background. The status bar shows **Autosaved** or an actionable warning if recovery storage is unavailable. Autosave never overwrites the source image or your manually saved project, and exporting a flattened image does not clear the editable recovery copy.

On the next launch, **Pick up where you left off** lists unfinished work with its name and autosave time. Choose **Recover** to open its original pixels, image edits and annotations for editing, **Discard recovery** to remove only that temporary copy, or **Decide later** and return through **App menu → Recover unfinished work**. A recovered document requires Save project / Save As to choose a permanent file; recovery does not silently overwrite its original. Undo history before the exit is not restored. Edits since the last completed autosave may be missing.

A successful project save, returning to the clean state with Undo, or explicitly discarding changes clears the current recovery copy. A normal quit retains any older recovery copies you deferred. Closing to the menu bar/tray keeps unfinished work protected. Each running instance locks its own draft, and only one latest snapshot is kept per unfinished session. The recovery folder is local application data (`Library/Application Support/com.MyImage.MyImage/Recovery` on macOS), separate from the configurable capture/project library, so moving your library does not remove recovery data. Demo runs use a separate temporary recovery folder.

## Build storage

Development and test builds disable incremental compilation and keep only file/line debug information for workspace code; dependency debug information is disabled. These settings apply to macOS and Windows builds. Compiled dependencies are still cached, but the large incremental caches and full dependency debug metadata no longer accumulate by default. Rebuilding changed code takes longer. Release optimization and signing settings are unchanged.

Every successful `cargo run -p xtask -- bundle` (release by default) automatically preserves the previous package in `backups/`, then cleans known macOS/Windows debug and release caches and obsolete MyImage QA app copies. It retains one rollback per platform/architecture, the new package in `target/bundle/`, small review screenshots, source snapshots and editable projects. New backups use `MyImage-rollback-macos-<arch>.zip` or `MyImage-rollback-windows-<arch>.exe`; copied bytes are verified before older backups are pruned. Repackaging the same numbered Mac release (or identical Windows executable) preserves the existing rollback. The very first release has no prior package to retain.

Failed builds/signing/packaging do not trigger cleanup. A failed backup stops cleanup, keeps existing backups, and preserves the previous package in staging with its location printed. A release lock prevents overlapping release jobs; active Cargo builds, running QA apps, linked folders, unrecognized paths and paths containing projects/user media are skipped. Skips are printed and retried after the next release. Cleanup never installs, quits or modifies `/Applications/MyImage.app`, and never visits your capture/project folders or signing credentials. A small running build-helper executable and empty Cargo lock directories can remain, particularly on Windows.

`--debug` packages go to `target/bundle/debug/` and do not trigger cleanup or replace the release package. Plain `cargo build --release` only compiles; use `xtask bundle` for the release-and-cleanup workflow. Complete dependency caches are removed after releases, so the next development/release build is a cold build.

`cargo clean` removes the entire `target/` tree, including packaged build outputs and review evidence. It does not remove `backups/`, the installed `/Applications/MyImage.app`, or the image library. Avoid cleaning while a build or a QA app from that directory is running.

## Capture and view

Capture home has visual **Region**, **Multi-region**, **Window**, **Full screen**, and **Scrolling** cards, a display selector, **Off / 3s / 5s / 10s** timer chips, an **Include pointer** toggle, and a prominent **Capture Now** panel. Rounded and elliptical region choices are no longer shown. Settings persist between launches.

The built-in **Capture Now** is also available from the menu bar/system tray, **Ctrl + Shift + .**, and on macOS **Command + Shift + .**. Click the menu-bar/tray icon to open a compact native **Capture Now** panel with mode tiles, display, Editor/Clipboard destination, pointer and capture-on-release switches, delay chips, and a round Capture button. Escape or clicking outside dismisses the panel. **Open Editor**, **Library**, and **More settings** provide direct navigation. Right-click retains the native menu; keyboard capture shortcuts still start capture directly. **Keep ready in background** hides the app while these entry points remain available. The header retains **Open…** and **Open project…**; Editor provides **Save as…**, **Export…**, and **Save project**.

Choose **After capture → Editor / Clipboard** in Capture settings, or change **To: Editor / Clipboard** in the Capture Now controls. **Clipboard** copies full-resolution pixels for pasting into another app, saves a PNG in Library, and closes the capture overlay without opening or replacing the Editor. Your existing document, unsaved edits and undo history stay intact. This also works for full-screen, window and scrolling captures; scrolling finishes with **Finish & copy**. The destination is remembered for later captures and hotkeys. Completion appears in MyImage's status bar and tray tooltip; clipboard failure retains the image and offers **Retry copy** or **Open capture in Editor**. The Editor destination asks about unsaved work before replacing it with the completed capture.

1. Select a capture type and adjust its options. Capture Now hides MyImage before taking the screenshot.
2. **Region:** click a highlighted window or drag a rectangle on the frozen screen, drag inside to move it or drag an edge/corner to resize it, then click **Capture** or press Enter. The magnifier shows original pixels at a consistent **4×** zoom, with pixel coordinates, while placing or adjusting the boundary. It waits for a fresh pointer event after the overlay opens and pads screen edges instead of stretching them. **Reset** lets you redraw and **Esc** cancels.
3. **Window:** point to a window to highlight it, click to select, then Capture/Enter. Space switches Region/Window inside the overlay; Tab cycles overlapping window candidates. Confirmation controls move near the completed selection. The native backend captures that window; closed/unavailable windows report an error. **Full screen** saves the chosen display directly after the selected delay.
4. **Scrolling:** click a window to select it, or drag over just its scrolling content. Move/resize the boundary before **Start scrolling**. Scroll down yourself at a steady pace; MyImage collects overlapping sections automatically. A small controller provides a preview, **Pause / Resume**, **Finish & edit**, and **Cancel**. Whole-window capture detects the moving content after its first reliable join; an area selection keeps your exact chosen bounds. Leave some content overlapping rather than jumping to a distant page. If alignment fails, scroll back a little and Resume; accepted sections are retained. Keep the window at the same size. Vertical scrolling is supported; the app does not scroll the target for you, and horizontal scrolling is not implemented.
5. Captures save as PNG in the dated library and go to the chosen destination: Editor or Clipboard. Include pointer uses the native cursor with its hotspot; turn it off for clean screenshots. Keep it off when the cursor could obscure scrolling content. Scrolling captures are bounded to 32,768 pixels per side and 100 megapixels.

On macOS, MyImage finds the typing key that produces `.` without modifiers in the layout active at startup and registers both shortcut combinations. Windows uses Control + Shift with `VK_OEM_PERIOD`. Shortcuts trigger on key release to avoid repeat captures.

The chosen display is used for shortcut captures, independently of the mouse position. If it is unavailable when displays are refreshed, MyImage selects the primary display or the first available display. Use **Refresh displays** after changing your monitor setup.

**Open…** reads PNG, JPEG, WebP, BMP, TIFF, GIF, and `.myimage` projects. Imported images can be annotated; existing text or shapes already flattened into their pixels cannot be separated into editable objects. Animated files currently open as a still image.

Captures default to the user's Pictures folder under `MyImage`, with paths shaped like:

```text
MyImage/YYYY/MM/DD/HHMMSS-nanoseconds.png
```

Dates use the computer's local time. A completed PNG is published without overwriting an existing capture. If saving fails, the image stays in memory and **Retry saving capture** is available in View only; choose a writable folder in Capture first.

**Capture immediately after selection** in Capture options is off by default. Turn it on to capture a region on mouse release or a window on click. With it off, adjust the boundary and choose Capture as before.

**Multi-region** combines up to 32 regions into one PNG, preserving their screen positions with transparent gaps. Drag in an empty area to add a region; drag a region or its handles to move/resize it. Hold **Command** on Mac or **Ctrl** on Windows while starting a drag to add an overlapping region, or enter Multi-region directly from Region mode. **Remove** or **Delete/Backspace** removes the selected region; **Reset** clears all. Finish with **Capture** or **Enter** (even when immediate capture is enabled). Editor and Clipboard destinations both support the transparent result.

## Browse the library

**Ctrl-click** (also **Command-click** on Mac) toggles multiple thumbnails in Library or Recents. **Shift-click** extends a range; **Ctrl/Command+A** selects the visible collection while a thumbnail has focus. Selection borders and counts are separate from the small dot marking the open image in Recents. **Delete/Backspace** or **Move to Trash…** opens a confirmation listing only those selected files. The OS Trash/Recycle Bin provides recovery; deleting an open file first prompts for unsaved changes. Search fields retain ordinary text-editing shortcuts.

Right-click a thumbnail for **Open in Editor**, **Copy**, **Rename**, **Duplicate**, **Favorites**, **File info**, and **Reveal in Finder/Explorer**. **Copy** puts one saved image on the system clipboard at full resolution, including transparency and the saved annotations/effects of `.myimage` projects. It runs in the background, confirms **Copied to clipboard**, and keeps the open document and Recents order unchanged. A selected batch stays selected when right-clicked; Copy is available for a single file. Duplicating a `.myimage` file retains its original and editable objects; renaming preserves its extension and never replaces another file. Recents also offers **Reveal in Library** for files inside the library folder and **Remove from Recents**, which hides entries without deleting files. Reopening a hidden file adds it back.

1. Choose **Library** in the top toolbar. It browses the same folder used for new captures, including images and `.myimage` projects in its subfolders.
2. Browse **All files**, **Images**, **Projects** or **Favorites**, then select a year, month or day. Collection counts, clickable breadcrumbs and Back/Forward make it easy to move between dates. Search narrows filenames within that collection and date.
3. Sort by **Date modified**, **Date created**, **Name** or **Size**. Click the active sort again (or its direction arrow) to reverse the order. Creation timestamps come from the filesystem; unavailable dates appear last. Switch between grid and list views, with S/M/L thumbnail sizes.
4. Double-click a thumbnail or choose **Open in Editor**. Project annotations stay editable. Hover a card to star it, or use its right-click menu. Favorites and display preferences are remembered.
5. Use **Refresh library** after changing files outside MyImage. Returning to Library and successfully saving a capture, project, or export also refreshes the listing. **Folder** selects the library and destination for future captures.

Project thumbnails include saved image edits and annotations. Browsing does not discard the image currently open in the editor; opening a different item prompts to save unsaved changes. Save projects beside their source captures to find both in the same dated folder.

Valid `YYYY/MM/DD` folders determine an item's date, so saving edits does not move a capture to another day. Files outside that structure use their last-modified date in the computer's local time. Choosing a different **Library folder** also changes where future captures are saved; existing files are not moved. Projects saved outside the selected library folder are accessible through **Open…** or by choosing their containing folder.

Scanning and thumbnail decoding happen in the background. Only visible thumbnails are requested, and the in-memory thumbnail cache is bounded. Symbolic links and hidden subfolders are skipped. Unreadable folders and failed previews show errors without preventing other captures from opening.

## Edit and save

Dialogs and prompts share the **Studio Contrast** design: charcoal headers, contextual icons, soft shadows and compact raised buttons. The same treatment covers export, image/canvas sizing, text, stamps, presets, library actions, storage, recovery and unsaved changes. Long content scrolls within the window and action rows wrap for larger UI text. File and folder pickers retain the native macOS/Windows interface.

Selected editable objects show a compact floating properties bar near their boundary and matching controls in the right inspector. Use swatches, visual style menus, presets and sliders. Arrows support solid/dashed/dotted lines and independent endpoint shapes/sizes. A slider drag creates one undo step; exact numeric entry remains optional. Arrangement stays in the inspector footer.

Home, Editor, Library, Capture and Create share one compact top toolbar; the annotation tools follow after a gap in Editor. The vertical sidebar is removed. Annotation buttons are 48 × 42 points. Everyday tools stay visible; **More** contains Blur, Pixelate, Simplify, Magic Wand, Crop, Cut Out, Pen, Highlighter, Step, Eraser, Magnify and Spotlight. **More → Customize toolbar…** pins, unpins and reorders tools; your choices persist and overflow stays in More. The bottom **App** menu contains View only, background/hide and Quit actions. The right inspector shows the current tool’s shape choices, quick styles, and properties; selection controls appear only when artwork is selected. Right-click the image (or use **Image actions** below it) for crop, resize, rotation, undo/redo, clipboard, and zoom controls.

1. Capture an image or choose **Open…** to start in Editor. **Open project…** filters the picker to editable `.myimage` files.
2. Choose **Text** and click the image. A floating **Edit text** window opens with a large writing area, font family (Sans serif/Monospace), size, color, italic, underline and alignment. Changes preview on the canvas; **Apply** saves one undoable edit and **Cancel** restores the previous text. Enter makes a new line; Command/Ctrl+Enter applies. Double-click existing text or choose **Edit text & formatting…** to reopen it. Choose **Arrow** or **Shapes**, then drag on the image. **Shapes** opens a compact picker in the right inspector. **2D** contains rectangle, rounded rectangle, ellipse, triangle, diamond, pentagon, hexagon, star, cross, and parallelogram. **3D** contains cylinder, prism, cone, sphere, hexagonal prism, torus and tetrahedron, drawn as editable projected diagrams with internal edges. The torus keeps its center transparent. Quick styles preview the chosen shape; color, stroke, and fill remain adjustable. Selecting an existing shape lets you change its type without losing its bounds or style, with undo support.
3. Choose **Select** to move an annotation or drag its handles to resize it. The annotation list selects overlapping objects; **Raise** and **Lower** change their stacking order. The text handle scales text size, and double-clicking opens its text editor.
4. Use **Undo** / **Redo**, or Command/Ctrl+Z and Command/Ctrl+Shift+Z while the canvas has focus. Delete removes the selected object. Arrow keys nudge it one image pixel; Shift+arrow keys move ten pixels. Text fields keep their normal text-editing shortcuts.
5. Choose **Save project** (Command/Ctrl+S). The first save opens a native file dialog in the storage folder’s `Projects` subfolder (normally `~/Pictures/MyImage/Projects`). Existing projects keep their location when saved; Save as starts beside that project. Use **App menu → Storage folder…** or **Library → Storage** to see the current paths, open them in Finder/Explorer, or change storage for future captures and new projects. After choosing a new folder, keep **Move existing captures and projects** checked and choose **Move and switch** to migrate the current library. Uncheck it and choose **Use folder** for future files only. Migration preserves date subfolders, favorites and Recents, gives conflicting files unique names, and updates an open project’s save location without discarding its unsaved edits. Only supported images and `.myimage` projects inside the current library move; external Recents, linked folders and other file types stay in place. **Save as…** (Command/Ctrl+Shift+S) makes another editable project. Reopen it with **Open…** to continue editing.
6. Choose **Export…** for a flattened image or PDF to share. Export does not replace project saving or clear the unsaved-edit indicator. Unsaved edits prompt before opening another file, capturing again, or quitting. Closing to the tray keeps the current project in memory.

Projects use a versioned `.myimage` container with image-edit settings, annotation JSON, and an embedded lossless original PNG. New saves use version 21; version 1–20 projects still open, with version 1 acquiring identity image settings. Older MyImage builds cannot open version 21 projects. Coordinates and sizes are stored in original-image pixels. Save operations publish a complete file atomically. Projects travel without sidecar images; undo history is session-only. Text supports Sans serif and Monospace from the bundled egui font set, italic, underline, color and paragraph alignment. Formatting applies to the entire text object; mixed formatting within a paragraph and installed system fonts are not yet supported. Unsupported glyphs may use font fallback.

Storage migration copies and verifies files on a worker, saves the new library/Recents references, and only then removes unchanged originals. Copy or preference-save failures keep the original library usable; any completed destination copies are retained and reported. Files changed externally before cleanup are retained at the source with a warning. Old empty directories can remain. Migration requires enough free space for verified copies and a separate destination outside the source tree; Windows runtime/cross-volume behavior is not yet natively tested.

With a drawing tool active, click an existing unlocked object to select it or drag it to move it; its floating controls and right-hand properties remain available. Draw on empty space to create another object. Drag drawings stay free of selection handles while drawing and after release; click the finished object to show its handles and edit it. Clicking empty image space or the surrounding canvas deselects objects without moving or refitting the image. Hold Alt/Option to draw over existing artwork. Shift supports multi-selection.

Draw in the empty workspace or continue beyond any canvas edge to add transparent space automatically. Moving or resizing objects and groups, placing text, stamps or step markers, and duplicating annotations can also grow the canvas. The expansion previews while drawing and commits with the object as one undo step; Escape cancels a drag. Original pixels and all object layers remain editable.

Closed shapes have eight small white resize handles: corners resize both dimensions, while side midpoints adjust only width or height. Handles have dark outlines for visibility on light or dark artwork, with larger invisible hit targets for easy grabbing.

For arrows and lines, drag the center diamond to bend the line, then drag the yellow points directly on the curve or either endpoint. Each yellow point follows your pointer while the opposite yellow point and endpoints stay fixed. **Straighten** removes the bend. These curve points remain editable after saving, copying, grouping and reopening.

## More editor tools

- **Magic Wand:** click a color, set tolerance (0–100%) and optionally select matching colors everywhere. Click again to recalculate. Use Copy, Move pixels, Delete pixels, Blur, Pixelate or Fill on the selected pixels. The saved operation retains the exact pixel mask.
- **Magnify:** drag a lens, choose Oval or Rectangle, adjust 1–8× zoom, border color and thickness. Select it later to move, resize or change its properties.
- **Spotlight:** drag a focus area, then adjust surrounding dimming and blur independently. The focus region stays clear. Shape, bounds and strength remain editable.
- **Simplify:** draw opaque editable blocks over details, with a color palette and oval/rectangle choices. **Auto simplify** finds text-like rows using a contrast-based detector and creates separate editable blocks. Detection is a starting point; review the result and adjust or delete individual blocks.
- **Cut Out:** choose Horizontal or Vertical, drag a strip and click Apply cut out. The remaining sides join and the output dimensions shrink. Up to 16 strips are stored in original-image coordinates; overlapping strips are combined. **Restore** removes a cut at any time, including after reopening. Image resize is cleared when applying a new cut. Original pixels and annotations remain intact.

All effects participate in project saving, undo/redo, layers, thumbnails and image/PDF export. Cut outs are image geometry, managed in the Cut Out panel. The project keeps original pixels; share a flattened export when you want only the edited result.

## Templates and recent work

**Create** offers a blank canvas, visual overview, two-, three- and four-step guides, before/after comparison, and a landscape walkthrough. Enter a title and choose **Create image**. Blank canvases also offer dimensions and background color. Template titles, captions, and image placeholders are separate editable objects. Select a placeholder and choose **Replace image / SVG…** in the right panel; imported artwork fits inside its bounds without stretching. Double-click captions to edit their text.

The collapsible **Recents** strip below Editor combines up to 24 recently used files with the newest captures from the library folder. It includes projects saved outside the library folder. Opening an existing thumbnail keeps the strip in place; completed edits, saves and new captures move the file to the front. Clicking another thumbnail with unsaved changes triggers the existing Save/Discard/Cancel prompt. Recents remembers file paths and their order, not separate unsaved editing sessions. Save a new template as a project to add it to the strip. **Refresh** updates external file changes.

## Stamps and signatures

Choose **Stamps** in the main toolbar. The right panel has search and six categories: **Symbols**, **Stickers**, **Activity**, **Business**, **Smileys and People**, and **Signatures**. Symbols, Stickers, Activity, and Smileys and People each include 30 built-in stamps; Business includes 40 (160 total). Drag a preview onto the image to place it, **double-click** to insert at the image center, or single-click a preview and then click the image. The gallery uses five compact columns and scrolls independently above a fixed property shelf. **Size** and **Opacity** set the next insertion; selecting an existing stamp shows its displayed-pixel **W/H**, a **Link** toggle for proportions, and opacity. Replace artwork and Edit SVG stay beside these controls; **More** holds selection, grouping, layout and privacy actions. A drag preview shows the drop location; Escape or dropping outside the canvas cancels. Insertions are undoable and remain editable, including SVG and signature stamps. Use Select to move or resize it; opacity, layers, grouping, duplication, clipboard, presets and privacy filters remain available.

**Signatures → + Add → Import image / SVG…** adds a local image to your reusable signature collection. Transparent PNG preserves a clear background. Imports accept SVG, PNG, JPEG, WebP, TIFF, BMP and GIF (first frame). Raster images are normalized to PNG, bounded to 2048 pixels per side and 1 MiB encoded; SVG imports keep their source. Signatures holds up to 64 entries; all custom collections together are bounded to 16 MiB; projects allow up to 3 MiB of embedded stamp payloads within the existing metadata budget. Imported images are embedded in each project and copied with annotations, so reopening does not depend on the source file or signature collection. Signature artwork remains an image object with editable position, size and opacity, rather than editable handwriting paths.

Built-in artwork is [Twemoji v16.0.1](https://github.com/jdecked/twemoji/tree/v16.0.1), by Twitter and contributors, under CC BY 4.0. [Attribution](assets/stamps/ATTRIBUTION.txt) and the [graphics license](assets/stamps/LICENSE-GRAPHICS) ship with the source and packaged application.

Use **+ Add → New category…** to create your own collection, then **Import image / SVG…** or **Design SVG…** to add artwork. Up to 32 custom categories can hold 128 stamps each, within the shared collection size limit.

**Design SVG…** provides a description field, **Copy AI prompt**, an SVG source editor, and a transparent preview. Paste the prepared prompt into any AI service yourself, then paste its SVG response into MyImage. Choose **Preview SVG**, then **Save stamp**. MyImage does not call an AI service or require an API key. **Edit SVG source…** reopens the source and description; selecting an inserted stamp allows an undoable artwork update. Other inserted copies remain independent. SVG source travels inside projects, annotation clipboard data and presets.

SVG stamps must be self-contained static artwork, at most 256 KiB, using paths, shapes, text and local gradients/masks. Scripts, external resources, embedded images, CSS, filters and `use` elements are rejected. Rendering uses Rust's resvg with bundled fonts, not a browser. Source remains vector-editable; canvas/export rendering rasterizes at a bounded resolution of up to 2048 pixels per stamp. Raster-image replacement keeps an embedded normalized image; SVG replacement keeps the SVG source.


## Select areas and objects

**Select** combines object clicks and area selection. Click an annotation to select it, then move or resize it. Drag across the image to create an area selection; **Rectangle** is the default and **Lasso** traces a freehand boundary. By default, dragging inside moves the selected pixels to the new location and leaves transparency behind. During a pixel move beyond any edge or corner, the moved pixels and transparent canvas extension appear live. The original image stays fixed at the current zoom while dragging; release commits the new canvas size. Moving back inside removes the temporary margin. The move and canvas expansion remain editable and Undo restores both together. Dragging an edge or corner adjusts only the selection boundary, without stretching the pixels. Choose **Adjust boundary** when you want interior drags to reposition just the selection for copying; **Move pixels** switches back. The panel also offers **Delete pixels**, **Blur**, **Pixelate** and **Fill**. Escape restores the previous selection during a drag; otherwise it deselects.

The Selection panel's **Width** and **Height** fields use the final copied-image pixels, including image crop, rotation and resize. Type a value or drag a field to adjust it; Copy uses the entered size immediately. **Lock aspect ratio** captures the current proportions and applies them to both fields and all boundary handles, with whole-pixel rounding. Enlarging near an image edge shifts the boundary inward when needed; dimensions remain inside the canvas. These fields adjust the selection only, even when starting from a saved effect, and preserve lasso/color masks. Creating a new selection, Select All or Deselect clears the ratio lock.

Blur and Pixelate are also available when an object or group is selected, and cover its bounding region in the composed image. Strength remains adjustable. A lasso restricts filtering and deletion to its boundary. Delete clears area pixels to transparency; Delete on an ordinary selected annotation removes that object.

Area moves and deletions are reversible operations in the layer stack. Moving samples the composition below the operation and clears its source; the original image and underlying annotations remain in the project. Later edits to lower layers are reflected in the moved content. Undo, layer visibility and **Remove edit** restore covered content. Area move/delete operations are tied to this image and cannot be copied into other images or saved as annotation presets. Regular shapes and annotations retain their existing copy/preset support.

## Fill backgrounds or make them transparent

Choose **Fill** in the main toolbar, pick a quick color or custom color, then click the background. The default fills connected, similar-colored pixels and stops at contrasting solid lines. Lower **Tolerance** protects edges; higher values include more color variation. **Global fill** also reaches disconnected matching regions. On a uniform canvas, one click fills the canvas. Make a rectangle or lasso selection first to limit either mode to that area; the selection boundary stays visible while Fill is active.

Choose the checkerboard swatch or **Transparent fill** to clear the region's pixels. **Opacity** supports partial transparency. PNG, WebP and TIFF preserve alpha; JPEG and PDF use the export background setting.

Each fill is an editable layer in the `.myimage` project. Its color and opacity update immediately. **Update fill region** recalculates from the original-resolution composition below that fill, using its saved click position, selection and current tolerance/Global setting. Use it after changing underlying artwork as well. The saved region stays fixed until you update it, so preview scaling cannot cause a fill to leak through a thin line. Cropping, resizing and rotating the image retain the fill and original pixels. Hiding or removing the fill restores the content below it; undo/redo includes fills. Fills are image-specific and cannot be copied between images or stored as annotation presets.

Boundary calculation runs in the background. A fill can store up to 65,536 scanline runs, with 131,072 across a project; very fragmented fills report a limit and leave the document unchanged. Reduce tolerance or select a smaller area if needed.

## Multi-select, groups, and duplicates

- **Multi-select:** use Select and Shift-click annotations on the canvas or in the Layers panel to add or remove them. Hold Shift and drag to select fully enclosed annotations; an ordinary drag selects image pixels. Use **Select all annotations** for all objects, or **Select all [type]** in the inspector for matching objects. Command/Ctrl+A selects the whole image for copying.
- **Group:** choose **Group** or Command/Ctrl+G. Clicking a group selects its members together. Drag to move them, use arrow keys to nudge them, or drag a corner handle to resize them proportionally. Lower, Raise, and Delete apply to the selected objects together.
- **Edit one member:** Alt/Option-click an annotation or its member row in Layers to change its text, color, thickness, or other properties without ungrouping. **Ungroup** or Command/Ctrl+Shift+G separates the selected groups into individual annotations.
- **Duplicate:** choose **Duplicate** or Command/Ctrl+D to create an offset copy of the selection. Copied groups have independent identities, so editing or moving a copy does not change its source. Duplicating an individually selected group member creates an ungrouped copy.

Groups remain editable after saving and reopening a `.myimage` project. Grouping preserves layer order and appearance, including privacy effects. Groups are flat: grouping existing groups merges their members into one group. Undo/Redo covers grouping, duplication, and collective edits during the current session. Exports and thumbnails show the resulting composition without selection outlines.

## Alignment, distribution, and snapping

Select annotations, then use **Align & space** in the inspector:

- **Left / Center / Right** aligns horizontal edges or centers within the selection’s bounds.
- **Top / Middle / Bottom** aligns vertical edges or centers.
- **Space horizontally / Space vertically** creates equal gaps between at least three objects, keeping the first and last in place. When the objects do not fit, the equal gaps can be negative (overlap).

A fully selected group counts as one object, preserving its internal layout. Alt/Option-selected individual members can be arranged separately. Directions follow the displayed image, including crop, rotation, and output resize. Each arrangement is one undoable edit, and the new positions stay editable when saved in `.myimage`. Layer settings are saved alongside these editable positions.

**Snap to guides** is enabled by default. While dragging an annotation or selection, magenta guides show alignment with image edges/centers and other visible annotations or complete groups. Snapping uses a six-screen-point tolerance at every zoom level. Hold **Alt/Option** during a drag to bypass snapping, or turn off the checkbox. Snapping applies to moving; drawing, resizing, and keyboard nudges retain their existing behavior. Guides are temporary editor overlays and are excluded from projects, thumbnails, and exports.

## Copy image pixels

- **Select all** (Command+A / Ctrl+A) selects the entire visible image. Find it beside Copy in the small dropdown, in the Selection panel, or in the image's right-click menu. A contrasting dashed boundary shows the selected rectangle or lasso; the footer reports the exact copy dimensions after image transforms. Escape or **Deselect** clears the selection. These boundaries never appear in copied, exported or saved pixels.
- **Copy** in the toolbar or **Command+C / Ctrl+C** copies the selected pixel area, or the whole visible image when no area is selected. Selecting an annotation alone still copies the whole image. All visible edits, including image geometry, are included. The duplicate Copy whole image action has been removed.
- Copy keeps the editor steady while composing the image. A quiet **Copied to clipboard** confirmation and pixel dimensions appear beside Copy for four seconds after success; the footer keeps the details. Slow copies show **Copying to clipboard…**. There is no full-editor dimming for Copy.
- **Cut** in the Selection panel or image menu (**Command+X / Ctrl+X**) copies the selected area first, then clears it to transparency as one undoable, editable layer. Use edges/corners or **Adjust boundary** to change the selection before cutting. Cut requires a pixel selection; use Select All for the entire canvas. Rectangular, lasso and color masks are retained, and the original image stays in the project. A failed clipboard write leaves pixels untouched; editing the document while a cut is pending cancels only its removal step. On Mac, both Command and Control work for image Copy/Cut/Paste; focused text fields retain their normal text shortcuts.
- **Select all annotations** remains a separate image-menu command (also **All annotations** in object selection controls). Text fields keep their normal Select All behavior.
- Draw a rectangle or lasso with **Select**, then use **Copy** or **Command+C / Ctrl+C**. Lasso and Magic Wand copies retain transparency outside the selection, including holes and disconnected color regions. Paste into any app that accepts images.
- **Command+V / Ctrl+V** on a MyImage canvas inserts clipboard pixels at their original size as a separate, centered image layer. The canvas expands with transparent space if needed. Drag an image layer beyond any canvas edge to expand it further when released. Move it, resize it, change opacity, hide it, or delete it without changing the original. Save as `.myimage` to retain the layer. In an empty Editor, **Paste image** creates a new document.
- Paste keeps the editor enabled while reading the clipboard. If the image changes during that read, it asks you to paste again instead of applying stale placement.
- Raster clipboard layers are bounded to 64 megapixels and 16 MiB of lossless PNG per layer, with a 32 MiB encoded artwork budget per project. Large pasted layers can be copied as pixels; editable annotation clipboard payloads retain their separate 4 MiB limit.

## Copy and paste annotations

Select annotations or a group in **Edit**, then right-click the image and choose **Copy editable annotations**. Open another image, enter **Edit**, and choose **Paste** or press **Command+V / Ctrl+V**. Text fields retain their usual text copy/paste behavior.

- Paste adds an independent, selected copy at the center of the destination’s current crop. Original-pixel dimensions, colors, opacity, text, font sizes, pen samples, callout pointers, step numbers, effect strengths, layer names, visibility, locks, and relative stacking order are retained.
- Destination image rotation and resizing apply to the pasted artwork as they do to other annotations. Large selections may extend beyond the destination canvas; their geometry is preserved.
- Copied groups receive fresh identities on every paste, including when the destination already contains groups. Copying one member copies just that annotation and retains its inherited visibility/lock state. Multiple selected members can retain their group together.
- Hidden or locked copies stay hidden or locked; Layers opens automatically so you can reveal or unlock them. Paste adds one undo step. Changing the pasted objects never changes the source image or clipboard snapshot.
- Save the destination as **`.myimage`** to reopen and edit the pasted annotations later. New shapes are retained in project version 9.

When copying selected annotation objects, the system clipboard carries a versioned MyImage text payload containing their editable data. It survives switching images; copying other content replaces it. Paste accepts MyImage annotation selections and validates their size and contents before changing the destination.

## Layers

Open **Edit → Layers** beside Styles. Compact rows show each object’s shape/tool icon and name, with eye and lock controls alongside it. Groups expand into member rows, with frontmost objects first. Select artwork to reveal Rename, Group/Ungroup, Duplicate and Delete; the small ordering controls move it through the stack. Hover an icon for its action.

- **Rename:** select an annotation or group and choose **Rename…**, or double-click its name. A blank name restores its automatic label.
- **Show:** hide or reveal the selected layer. Hiding a group preserves each member’s own visibility setting. Hidden layers stay editable in the project and are omitted from the canvas, viewer, thumbnails, and every export format.
- **Lock:** protect a layer against moving, resizing, appearance changes, erasing, deletion, grouping, and reordering. Group locks protect every member. A collective edit containing any locked member is blocked; select an unlocked member row to edit it separately. Names and visibility remain adjustable while locked.
- **Order:** use **Lower / Raise** to move one stack position, or **To back / To front**. Selecting a group moves its members together while retaining their relative order.
- Click a group row to select its members, or a member row to select just that annotation. Shift-click adds or removes layers from the selection. Hidden and locked layers remain selectable here.

Every layer change supports undo and redo. Saving and reopening `.myimage` retains names, individual and group visibility, locks, stacking order, and editable artwork. Export produces a flattened copy; keep the project to make later changes. Global crop, rotate, and resize still affect the whole image, including locked layers.

## Quick Effects and compact workspace

Open **Effects** beside Styles, Layers and Presets, or use **Effects** in the canvas footer. Visual cards apply **Soft shadow**, **Floating**, **Ink border**, **Calico border** and **Grayscale**. Shadow opacity, softness and offset use sliders; Border offers width, a color picker and swatches. The Shadow, Border and Color tabs show only the chosen properties. Effects style the entire composition while retaining the original image and editable annotations in the project.

Borders and rectangular frame shadows add transparent canvas space around the current image frame. Changing effects reuses that frame, preventing cumulative padding. **No effects** removes styling; the extra canvas remains available. Undo restores both styling and canvas size. Preview, copying, Library thumbnails and every export format include the effects. Grayscale preserves alpha and leaves the border’s independently chosen color intact.

The footer exposes **Fit** and a clickable zoom percentage with common sizes and a slider. These change the view only. Recents keeps its show/hide toggle, uses one compact header and smaller thumbnail cards, and shows selection actions only when files are selected. Refresh and help use icons; full filenames and paths remain in hover tips. Ctrl/Command-click, Shift ranges, right-click actions and stable opening order are unchanged.

## Image and canvas tools

Open **Image ▾**, beside **Save as…** in the Editor header. The same operations are available by right-clicking the canvas. They affect the image and its annotations together while preserving the original pixels and separate annotation objects.

- **Crop:** drag a rectangle on the canvas, then apply it. Cancel leaves the project unchanged. Cropped-out pixels and annotations remain in the project and can be restored with **Restore full image** in the crop inspector.
- **Resize image:** enter the output pixel dimensions. **Lock aspect ratio** is enabled initially; turn it off to stretch the image. Apply commits one undoable edit.
- **Canvas size:** shrink or enlarge the visible canvas at exact output dimensions without scaling the artwork. Choose an anchor: Top left changes the right/bottom edges; Center distributes changes around the image. Content outside the canvas is hidden, not deleted; enlarge it again to reveal the original pixels and editable objects. Pasting or dragging an image layer beyond an edge also expands the canvas. Canvas growth and layer placement share one undo step. In Fit mode, moving a pixel selection past an edge fits the whole growing canvas into view while the mouse is held. Pinch in/out over the Editor or View-only canvas to zoom around the pointer; Control-scroll also works. Pinching switches from Fit to manual zoom without editing the image.
- **Rotate left / right / 180°:** rotate the entire composition. A quarter-turn swaps the output width and height.
- **Flip horizontal / vertical:** mirror the whole composition along the displayed axis. Original pixels and individual annotation objects remain editable.
- **Reset image edits:** restore the full original size and orientation without deleting annotations. Undo can restore the previous image settings.

**Save project** keeps these settings editable after reopening. Repeated resizing always starts from the original image rather than an already resized copy. Annotations stay anchored in original-image coordinates and follow the crop, rotation, and resize. Hidden annotations remain available in the Layers panel. Viewer previews, project thumbnails, and all export formats use the edited result.

Projects preserve the original crop, transparent canvas margins, cut-out strips, quarter-turn rotation, horizontal/vertical mirroring, and output resizing. New projects use format version 17 (including reversible canvas windows and editable arrow endpoint and line styles); earlier versions remain readable. Output dimensions must be nonzero, at most 32,768 pixels per side, and at most 100 million pixels in total. Exported files contain the finished result; keep the `.myimage` file to revise the original or annotations later.

## Editor tools and quick styles

Edit keeps annotation tools in a top toolbar. Selecting **Arrow**, for example, opens **Arrow → Quick styles → Tool properties** in the right panel. The **Image ▾** menu beside Save as contains whole-image and canvas operations. Right-click the displayed image for these operations plus Undo/Redo, Copy/Cut/Paste and zoom controls. The small **Image actions** menu below the canvas offers the same commands. Choose a visual style, then draw; selecting an existing annotation lets you restyle it in one undoable action. Styles adjust the relevant colors, thickness, fill, text size, or effect strength while keeping the object's content, geometry, and group/layer settings. Locked artwork cannot be restyled, and multi-selections retain their arrangement controls. Text, arrows, shapes, steps, callouts, highlighters, pen, blur, and pixelate each have contextual choices; Eraser has its own size controls.

**Layers** and **Presets** stay available beside Styles. Choosing a drawing tool returns to its properties automatically. The compact inspector shows only that tool’s options until artwork is selected; grouping, alignment, duplicate and deletion controls appear with a selection. Quick styles are built-in starting points; use Presets to save your own reusable artwork. Inserted or restyled annotations remain editable in `.myimage` projects. Zoom, opacity, privacy strength, and JPEG quality use a shared slider with a thin track, small white thumb, and a larger invisible interaction target. Opacity, privacy strength, and JPEG quality also have editable numeric fields.

## Reusable annotation presets

In **Edit → Presets**, select annotations or a group on the canvas or in Layers, then choose **Save selection as preset…** and enter a name. The compact, searchable two-column gallery shows artwork previews, names and object counts. Double-click a card, or select it and choose **Insert preset**, to add an editable copy to the current image; **Rename…** and **Delete…** manage the saved library. Deletion asks for confirmation and leaves inserted copies intact.

Presets retain annotation types, text, colors, thicknesses, freehand paths, callout pointers, group membership, layer names, visibility, and locks. Each insertion creates independent groups, selects the new artwork, and can be undone in one step. Hidden or locked copies open in Layers so you can show or unlock them. Copies are centered in the destination crop at their original pixel sizes; the destination's rotation and resize apply normally. Very large presets may extend beyond a smaller image. Editing a copy does not update the preset.

Save the destination as a **`.myimage` project** to retain the inserted annotations as editable objects. The project contains the objects themselves and does not depend on the preset library. The local library persists across app restarts in the platform application-data folder as `annotation-presets.json`; open the help icon and hover **Saved on this computer** to see its path. Demo mode uses a separate library in the temporary `MyImage-demo` folder. **Refresh** picks up external changes. The library supports up to 128 presets and 16 MiB total, within the existing per-project annotation limits. Presets contain annotations only, without source image pixels. Blur/pixelate presets show a descriptive preview because their appearance depends on the destination image.

## Steps, callouts, and highlighting

- **Step:** click to place numbered markers in sequence. The inspector changes a selected marker’s number, size, fill color, and number color. Moving, deleting, or reordering a marker preserves its number; new markers continue after the highest existing number.
- **Callout:** drag a speech bubble, then edit its text in the floating text editor. Move the bubble and its pointer together, resize the bubble with the corner handles, or drag the pointer tip independently. Text wraps and scales down to fit the bubble; the inspector controls its preferred size and colors. Enlarge the bubble or shorten the text if it cannot fit.
- **Highlight:** drag a translucent rectangle over text or an area. Adjust its color and opacity in the inspector. Overlapping highlights combine in layer order.

These are separate editable annotation objects. Select, move, resize, raise/lower, delete, undo, and redo work as with existing annotations. They follow image crop, rotation, and resize, appear in thumbnails and all export formats, and remain adjustable after saving and reopening a `.myimage` project. Blur or pixelate layers above them also obscure them.

## Freehand pen and eraser

- **Pen:** drag to draw a stroke, or click for a round dot. Adjust the pen color and thickness in the inspector. Pen stays active so you can draw several strokes in succession.
- **Select:** choose a stroke on the canvas or in the Layers panel to change its color or thickness, move it, resize it with its handles, or change its layer order.
- **Eraser:** choose a size and drag across pen marks. It removes touched sections of freehand strokes; surviving pieces remain separate editable strokes. The original image and other annotation types are unaffected. One Undo restores an erasing gesture.

Pen paths follow image crop, rotation, and resize and appear in previews, thumbnails, and all exports. Save a `.myimage` project to retain the paths, colors, and thicknesses for editing after reopening. Undo/redo history lasts for the current session.

## Blur and pixelate

Choose **Blur** or **Pixelate** in Edit, then drag a rectangle over the area to obscure. The inspector adjusts **Blur strength** or **Block size** from 2–128 original image pixels. Use Select to move a region or drag its corner handles to resize it. The annotation list selects overlapping regions; Lower, Raise, Delete, Undo, and Redo work as with other annotations.

Effects act on the image and annotations below them. An annotation above an effect remains visible. Crop, rotation, and resize apply afterward, keeping every region attached to the original image. Save a `.myimage` project to retain the region, effect type, strength, and stacking order for later editing.

The editor displays a bounded current composition immediately and refines it in the background, avoiding dark flashes after paste or during pixel moves. It never reuses an outdated cut/redaction preview or exposes raw source pixels. The view-only mode still uses a neutral placeholder while its background preview loads. Previews and thumbnails are bounded approximations, while exports apply effects at full resolution. Inspect the exported copy at full size before sharing: blur and pixelation obscure detail but do not guarantee unrecoverable redaction. **Share the exported image or PDF; the editable project retains the original sensitive pixels and annotations.**

## Export images and PDF

Choose **Export…**, select a format and its options, then choose **Choose destination…** to save through the native file dialog. The suggested filename ends in `-annotated` so the export is separate from its source. Raster exports use the edited output pixel dimensions; PDFs embed those same full-resolution pixels.

| Format | Options and behavior |
| --- | --- |
| PNG | Lossless RGBA with transparency. |
| JPEG (`.jpg` or `.jpeg`) | Quality from 1–100, default 90. Transparent pixels are composited against the selected background color, initially white. |
| WebP | Lossless RGBA with transparency. Each side must be at most 16,384 pixels; choose another format for larger images. |
| TIFF (`.tiff` or `.tif`) | Lossless Deflate compression with explicitly labeled straight alpha for transparency. |
| PDF | One page with **Image size**, **A4**, or **US Letter**. Image size uses 96 dpi, proportionally scaling pages that would exceed PDF's size limit. A4 and Letter fit the entire image within margins and automatically choose portrait or landscape. Image transparency is preserved. |

All exports apply the image edits and combine annotations into the image pixels. Use **Save project** to keep the original pixels and separate editable annotations in `.myimage`; exporting neither changes that project nor marks unsaved edits as saved. Canceling export keeps the current document, and a failed export does not replace an existing destination file with partial data.

Exported raster images inside the selected library folder appear after the library refreshes. **PDF is export-only**: PDF files are not imported by Open or listed as library thumbnails. Keep the corresponding `.myimage` project for further editing.

## Background behavior and permissions

Closing the main window or selecting **Background** keeps MyImage running in the menu bar/system tray. Choose **Open MyImage** to return. **Quit** in the window or **Quit MyImage** in the tray menu exits and releases the shortcut. If tray initialization fails, MyImage keeps the window available instead of hiding it.

If another application owns a capture shortcut, MyImage identifies the unavailable chord. Any successfully registered shortcut keeps working, as do the button and tray menu. Free the conflicting shortcut in the other application and select **Retry unavailable shortcuts**; this only retries failed registrations.

On macOS, restart MyImage after changing keyboard layouts. If the active layout has no supported unmodified period key or supplies no readable layout data, MyImage reports that the shortcut is unavailable; it does not register an extra fallback chord. The button and tray menu remain available.

On macOS, an explicit capture action lets ScreenCaptureKit perform its authorization check and may open a system consent dialog. **Allow screen capture…**, when shown, also requests access. Enable MyImage under **System Settings → Privacy & Security → Screen & System Audio Recording** (called **Screen Recording** on some releases), then quit and reopen the same app bundle if macOS requests it. Running through `cargo run` can associate access with the terminal or launcher; use the app bundle for normal testing. Startup and display enumeration do not request permission. The permission indicator is a hint; a successful capture is the end-to-end check.

Use the same persistent signing certificate for every installed update. The older ad-hoc build's authorization does not transfer to the new certificate: that transition needs one fresh grant. If an old grant remains stale despite permission toggling and a restart, a targeted reset followed by a fresh grant may be needed. **Obtain the user's explicit consent before resetting permission**, because this removes MyImage's saved Screen Recording decision. With MyImage quit, the manual command is:

```sh
tccutil reset ScreenCapture com.myimage.app
```

Reopen `/Applications/MyImage.app`, explicitly request capture, grant permission in macOS, and restart if requested. This command targets MyImage for the current user; it needs no `sudo`. Permission resets must never run automatically during startup, builds, or packaging. See Apple's [resetting access to protected resources documentation](https://developer.apple.com/documentation/xcode/resetting-access-to-protected-resources-in-macos).

macOS capture uses **ScreenCaptureKit's `SCScreenshotManager`**, which requires macOS 14 or later. Windows uses **GDI `BitBlt` through `xcap 0.9.8`**. `xcap` also supplies display metadata on both platforms. There is no legacy CoreGraphics screenshot fallback on macOS, and Windows Graphics Capture is not implemented. Capture errors are reported; permission failures do not produce substitute blank images. Secure/protected content and HDR fidelity are not guaranteed by this milestone.

## Development and validation

```sh
cargo test
cargo run -- --demo
```

`--demo` substitutes a generated 1600 × 1000 test image for screen pixels. It exercises selection, masks, saving, and viewing without requesting Screen Recording access. Every demo launch starts with isolated defaults and an output folder at the system temporary directory's `MyImage-demo` subfolder; it does not load or persist normal settings. Demo success does not validate real capture, monitor placement, permissions, or global shortcuts.

See [release validation](docs/RELEASE-VALIDATION.md) for the manual desktop checks and verification record.

### Play with Calico

Open the bottom-right calico App menu and choose **Play with Calico** for a five-second surprise: running, hopping, zoomies, peeking or chasing yarn. Each play chooses a different act. Press Escape to stop early. The mascot is an input-transparent window overlay; it never becomes part of your image, clipboard or exports. Starting capture, hiding the app or opening a modal stops the animation. Menu items have matching contextual icons.

## Releases, installation and updates

Get official packages from [GitHub Releases](https://github.com/gokcol/MyImage/releases), also linked in the calico App menu. The releases include an Apple Silicon macOS build and a Windows x64 build labeled **UNTESTED**. Windows artifacts are not code-signed and have not had interactive capture/clipboard/installer testing.

On Mac, quit MyImage after saving, open the disk image, and drag MyImage into Applications. Keep the same location and bundle identifier for upgrades. To uninstall, quit the menu-bar app and move `/Applications/MyImage.app` to Trash; captures, projects and local preferences/recovery remain intact. The app does not install a background service or login item. **Quick start & shortcuts** in the App menu reopens the three-step guide.

For maintainers, `cargo run -p xtask -- install` installs the prepared local bundle transactionally: it requires a newer build, verifies the distributor signature and Gatekeeper acceptance, refuses running apps, byte-verifies the staging copy and rollback ZIP, and restores the previous app on failed verification. It preserves one rollback ZIP under `backups/` and never changes user data or TCC permission records. `--allow-signing-migration` explicitly permits the one-time transition from local development signing to the official Developer ID; macOS may require a fresh Screen Recording grant for that transition. Future releases keep the same Developer ID. Updates are downloaded deliberately through GitHub Releases; the app does not silently download or replace itself.

Public macOS packages use `MYIMAGE_DISTRIBUTION=1 MYIMAGE_SIGNING_IDENTITY=<certificate-fingerprint> cargo run -p xtask -- bundle`, followed by `cargo run -p xtask -- notarize <existing-keychain-profile>`. This enables the hardened runtime and timestamp, submits to Apple, staples and validates the ticket, assesses Gatekeeper acceptance, and rebuilds the ZIP with the stapled app. Credentials stay in the macOS Keychain and must never be committed or uploaded to GitHub.

### Direct canvas handles

The post-capture **Selection** panel uses illustrated Rectangle, Lasso, Select all and Deselect tiles. Selected areas expose compact width/height fields, an aspect-lock icon and visual Boundary/Pixels drag choices. Copy, Cut, Delete, Blur, Pixelate and Fill are grouped as icon tiles; unavailable actions stay disabled. Hover for shortcuts and details, or open the panel's information button for selection help. Cut and Delete still leave transparency, and edits retain their existing layer and undo behavior.

Eight small square handles surround the canvas in Editor. Drag a corner or edge midpoint to resize the canvas with a live dimension readout. Hold **Shift while dragging a corner** to preserve the canvas aspect ratio with the opposite corner fixed; Shift can be pressed or released during the gesture. Edge midpoints still resize one dimension. Artwork keeps its size: new space is transparent and shrinking hides content without deleting it. Grow the canvas again to reveal it, including after saving and reopening a `.myimage` project. Press Escape to cancel; a completed drag is one Undo step. Deselect a pixel region to reveal canvas handles, so region handles retain priority.

Recents and cached Library thumbnails check file availability in the background while visible. Files deleted externally are removed from navigation and selection automatically, including persisted recent references. Missing-file actions clear stale entries without repeated filesystem errors. This never closes the in-memory image or discards unsaved edits; unreadable or corrupt files are not treated as missing.

Quick Effects includes reversible torn, wave, sawtooth and rounded edges, with depth/radius and individual side controls. Borders and shadows follow the styled edge. The compact Recents header also contains icon-based image tools, Fit, zoom percentage and Effects. Library uses a compact collection/date sidebar and a search/sort header; the calico App menu groups shortcuts and interface-size settings in a compact Studio Contrast panel.

Effect Styles also includes Perspective, Reflection, Fade to transparency, Sepia/Warm/Cool/Invert filters, and brightness/contrast/saturation adjustments. These compose without flattening layers and are saved in project version 21. App → About MyImage credits Orhan Gökçöl and shows version/build details.

The Recents filmstrip uses an icon-only chevron to expand or collapse, with Show/Hide recents tooltips.

Line styles use icon previews in the floating controls and properties panel: Solid, Dashed, Dotted, Dash-dot and Long dash. The same editable patterns apply to 2D and 3D shape outlines, and remain intact in projects and exports.
