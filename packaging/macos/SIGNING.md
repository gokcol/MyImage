# Persistent signing for local MyImage builds

macOS associates privacy grants with a signed application's designated requirement.
An ad-hoc signature identifies one exact build. A rebuild, icon update, or bundle
metadata change can therefore invalidate a previous Screen Recording grant.
MyImage's default packaging now requires a persistent certificate identity and
never silently falls back to ad-hoc signing.

## Existing Apple signing certificate

Use `security find-identity -v -p codesigning` to find its certificate fingerprint.
Set `MYIMAGE_SIGNING_IDENTITY` to that 40-character fingerprint when running
`cargo run -p xtask -- bundle`. Alternatively save the fingerprint as a single line
in `~/Library/Application Support/MyImage/signing-identity.txt`. That file contains
only a public identifier, never a password or private key.

## Local development on this Mac

If no Apple identity is available, the explicitly invoked Rust helper can create
a dedicated **MyImage Local Development** self-signed code-signing certificate:

```sh
cargo build --release
cargo run -p xtask -- setup-local-signing --confirm
cargo run -p xtask -- bundle
```

The opt-in setup creates a ten-year RSA certificate, imports its private key into
the user's default Keychain as non-exportable, and allows `/usr/bin/codesign` to use
that key. Temporary private-key files are restricted to a private temporary
directory and removed when setup exits. Only the public certificate and fingerprint
are saved under MyImage's Application Support directory. Setup refuses to replace
an existing identity and verifies a signed probe before configuring builds.

It does **not** add a trusted root, disable Gatekeeper, alter TCC, request Screen
Recording, or change other apps' permissions. This is a local development identity;
public distribution still requires an Apple Developer ID and notarization. If
Keychain asks for authentication, the user must approve the system prompt.

Keep the certificate and key for subsequent updates. Do not generate a new one
on every build. If an import succeeds but signing is interrupted, reuse that
certificate's fingerprint; do not run setup again to create another identity.

## Installation and the one-time transition

Quit MyImage normally, preserving any unsaved document. Install the complete signed
bundle at `/Applications/MyImage.app`. Launch that exact copy. Transitioning from
the old ad-hoc identity to the persistent identity requires one fresh Screen &
System Audio Recording grant, followed by Quit & Reopen if macOS requests it.
Subsequent builds signed by the same certificate satisfy the same designated
requirement. Do not modify a signed bundle after packaging.

If the old grant remains stale, ask the user before a targeted
`tccutil reset ScreenCapture com.myimage.app`. Never reset grants in a build or at
app startup, and never reset any other app's permissions.

Disposable builds can explicitly use `bundle --ad-hoc`; these are unsuitable for
normal installation because their privacy identity changes on rebuild.
The macOS CI job runs formatting, lint and unit checks without creating or
importing signing credentials on a hosted runner. Public signing stays on the
maintainer's Mac.

## Calico icon

`cargo run -p xtask -- icons` uses Apple's `sips` and `iconutil` on macOS to generate
all native icon sizes (16 through 1024 pixels) from the original transparent logo.
It writes `assets/branding/MyImage.icns` for Finder/Applications and a 256-pixel PNG
for the running app's Dock/window icon. In restricted build environments, iconutil
may need execution outside the sandbox. Generated assets are kept with the source,
so normal builds do not need iconutil.

References: [Apple's designated requirements](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements),
[code signing identities](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html).

## Public Developer ID releases

Use a valid **Developer ID Application** identity and `MYIMAGE_DISTRIBUTION=1`
when bundling to enable the hardened runtime and trusted timestamp. A local
self-signed certificate is only appropriate for development on its owning Mac.

```sh
MYIMAGE_DISTRIBUTION=1 MYIMAGE_SIGNING_IDENTITY=<SHA-1-fingerprint> cargo run -p xtask -- bundle
cargo run -p xtask -- notarize <existing-notarytool-Keychain-profile>
cargo run -p xtask -- install
```

The notarization command submits the ZIP, staples/validates the application,
checks Gatekeeper acceptance, and recreates the ZIP containing the ticket.
It does not read or print the password behind the Keychain profile. Never
commit credentials or export a private signing key into the repository.

The installer requires the official MyImage distributor's Developer ID,
a newer build and Gatekeeper acceptance. The first upgrade from a local
certificate needs the deliberate `--allow-signing-migration` option. This
can cause macOS to require a fresh Screen Recording grant; do not reset
TCC automatically. Subsequent releases retain the same Developer ID and
bundle identifier. `--allow-unnotarized` exists only for explicit local
troubleshooting and must not be used for a normal public release.

## Signed disk image

After notarizing the app, create a DMG without rebuilding or modifying it:

```sh
MYIMAGE_SIGNING_IDENTITY=<SHA-1-fingerprint> MYIMAGE_NOTARY_PROFILE=MyPDF \
  sh scripts/package-dmg.sh target/bundle/MyImage.app dist/installers
```

This creates `MyImage-<version>-build<N>-macos-<arch>.dmg`, its SHA-256 checksum
and Apple's submission receipt. The image contains the same signed/stapled app,
an Applications link and concise installation instructions. The script requires
the official Developer ID and notarized source app, signs and notarizes the DMG,
validates its ticket/Gatekeeper status, mounts it read-only, verifies the embedded
app and link, and detaches it before publishing files. Existing output is refused.

The **macOS signed and notarized DMG** workflow performs the same packaging on a
hosted Mac using the ZIP already attached to the selected GitHub release. The
ZIP checksum is checked first, and the workflow never rebuilds the application.
Hosted signing requires these GitHub Actions settings:

- Secrets: `MACOS_CERTIFICATE_P12_BASE64`, `MACOS_CERTIFICATE_PASSWORD`, `APPLE_NOTARY_KEY_P8`.
- Variables: `MACOS_SIGNING_IDENTITY` (certificate SHA-1), `APPLE_NOTARY_KEY_ID`, `APPLE_NOTARY_ISSUER`.

Hosted credentials are not configured automatically and are never copied out of
the maintainer's Keychain. Until they are configured, use the local command above
and attach the DMG/checksum with `gh release upload`. The workflow fails clearly
if its signing configuration is missing. Temporary hosted signing material is
removed even if packaging fails; no unsigned DMG is published as a fallback.
