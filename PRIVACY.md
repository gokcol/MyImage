# MyImage privacy policy

Effective date: 13 November 2026

MyImage is a desktop screen-capture and image editor by Orhan Gökçöl.
This policy describes version 2.2.2 and applies to its GitHub distribution
and any Microsoft Store edition using this source.

## On-device processing

MyImage processes screen captures, images, editable projects, SVG stamps and
signature artwork on your computer. Screen content can contain personal or
confidential information. You choose what to capture, open, copy, save or export.
The app does not upload that content to the developer or a MyImage server.
Extract text uses the operating system’s on-device text recognition on your
selected region, or the whole visible image when no region is selected.
Recognized text is held in memory for review and is
placed on the clipboard only when you choose Copy text. Capture Now’s Text mode
copies recognized text directly after your screen-region capture action, without
saving an image or project. MyImage does not upload the selected pixels or
recognized text.

It does not require an account and contains no advertising, analytics or
automatic crash-report uploads.

Capture requires the permissions provided by your operating system. Clipboard
actions place the image or selected content on the system clipboard, where other
applications and any clipboard-sync service you enable may access it.
The SVG stamp designer copies a prompt for you to use with an AI service of your
choice; MyImage does not send that prompt or your images to an AI service itself.

## Local files and recovery

MyImage stores preferences, recent paths, favorites, custom stamps, signature
artwork and presets locally. Autosave stores separate recovery copies of
unfinished work, including image content and editable annotations. Recovery
copies can therefore contain sensitive information. You may recover or discard
them through the app.

Captures and projects default to `Pictures/MyImage`; you can change the folder.
On Windows, preferences are under `%APPDATA%\MyImage\MyImage\config` and local
assets/recovery under `%LOCALAPPDATA%\MyImage\MyImage\data`. On macOS these use
`~/Library/Application Support/com.MyImage.MyImage`. Temporary files may be
created during editing, capture and export. The app does not separately encrypt
these files; protection depends on your OS permissions and disk encryption.

Uninstalling the app does not intentionally delete captures and projects saved
in Pictures/MyImage or another folder you choose. The Microsoft Store edition
uses standard MSIX app-data handling: newly created preferences, stamps and
recovery files may be stored privately by Windows and removed on package
uninstall or reset. Save unfinished work as a .myimage project before removing
the Store edition. The installer/portable edition does not intentionally delete
its shared application data on uninstall. Delete unwanted files yourself or use the app's
Move to Trash/recovery-discard controls. Backups, network drives and cloud-synced
folders you use are governed by those services and your settings.

## Update checks

What's new & updates displays bundled release notes without a network request.
Manual update checks on macOS contact GitHub's public release API. GitHub receives
ordinary connection information such as your IP address and the app's update-check
user agent. Store-installed Windows copies query Microsoft Store services using
their package identity. Images, project contents, recognized text and file paths
are not sent with these checks. Optional daily background checks are off by default.
You can enable or disable them in What's new & updates. Check times, the last
viewed release and dismissed update notices are stored locally.

Update buttons open a GitHub Mac download in your browser or the Microsoft Store
listing. MyImage does not automatically download or install an update itself.

## External links

Opening source, release, attribution or privacy links uses your web browser.
GitHub or the destination site then processes the request under its own privacy
policy. MyImage does not attach image contents to those links. Store download,
update and diagnostic processing is governed by Microsoft's policies.

## Contact and changes

For privacy and support questions, email
[orhan.gokcol@hotmail.com](mailto:orhan.gokcol@hotmail.com).
Do not include confidential images or personal information in public issues.
Changes to this policy will be published here with an updated effective date.
