# Windows GitHub installers

The **Windows installer and portable ZIP** workflow builds the app from the
specified existing release tag, then attaches `MyImage-<version>-build<N>-Setup.exe`,
the portable ZIP, SHA-256 checksums and a source/signing manifest. Leave the tag
empty for Actions artifacts only. Packaging tools come from the workflow commit;
the manifest records both application and packaging commits. Existing assets
are never overwritten silently.

These downloads are **unsigned**, as requested to match MyPDF. The earlier
production-signature-only publishing rule was replaced explicitly for this
channel. They must not be described as trusted or Microsoft-signed downloads.
Windows may show an unknown publisher/SmartScreen warning. Do not disable security
protections. Store MSIX signing/distribution remains separate; no test certificate
is shipped. Production Authenticode signing can be added when a provider is configured.

Setup uses Inno Setup 6, installs for the current user into
`%LOCALAPPDATA%\Programs\MyImage`, and does not require administrator privileges.
It creates a Start menu shortcut and an Add/Remove Programs entry. Desktop and
Open with integration are optional; default image associations are not changed.
Startup remains off by default. Uninstall removes a login entry only when its
command points exactly to this installation. Projects, original images, settings,
recovery, unknown files and other copies' startup entries are preserved.

Save edits and quit the app, including its tray process, before updating or
uninstalling. Setup never requests a forced close. The portable ZIP contains the
same executable, icon, notices and licenses as Setup.

Local Windows build:

```powershell
./scripts/package-windows.ps1 -OutputDirectory dist/windows
```

Requires Rust from `rust-toolchain.toml`, Visual Studio C++ tools, and Inno Setup 6.
CI validates install, repeat installation, binary identity, shortcuts, Open with
quoting/cleanup, login startup and uninstall with project preservation on a
disposable Windows runner. This does not replace interactive capture, clipboard,
touch/pen, display scaling or tray tests.
