# Microsoft Store preparation

This follows MyPDF's MSIX approach: a native desktop app, an exact Partner Center
identity, SDK validation and a separate Store submission artifact. It is not a
Store listing or certification approval. Windows interactive validation remains
pending, including real capture, clipboard, DPI, permissions and tray behavior.

## Signing

The Mac GitHub release is Developer ID signed and notarized. Windows interactive
validation remains pending. GitHub also offers an explicitly **unsigned** Inno
Setup installer and portable ZIP, matching MyPDF's direct-download approach.
Their packaging/install/uninstall checks are separate from this Store pipeline.
See [Windows installers](../README.md). These direct downloads are not
Microsoft-signed and may trigger Windows publisher/SmartScreen warnings.

Microsoft signs an MSIX after Store submission/certification. The unsigned MSIX
produced here is an upload artifact, not a direct GitHub installer. Test identities
and temporary self-signed certificates must never be shipped to users as trusted
release signatures.

## Product identity and build

Reserve MyImage's name in Partner Center. Copy these **public identity values**
from Product management → Product identity to the repository's Actions variables:

| Variable | Exact Partner Center value |
| --- | --- |
| `MSIX_IDENTITY_NAME` | Package/Identity/Name |
| `MSIX_PUBLISHER` | Package/Identity/Publisher, including `CN=` |
| `MSIX_PUBLISHER_DISPLAY_NAME` | Package/Properties/PublisherDisplayName |
| `MSIX_DISPLAY_NAME` | Reserved product display name |

Do not reuse MyPDF's package identity or invent a publisher. In Actions, run
**Microsoft Store MSIX — validation**, select the release tag and `store` mode.
Without identity values use `test` mode, which deliberately produces TEST-ONLY
artifacts. The workflow does not submit to Partner Center.

The registered MyImage identity supplied from Partner Center is:

- Package name: `OrhanGkl.MyImageOG`
- Publisher: `CN=A9F0208B-CC43-472D-AE82-9FE4556DC935`
- Publisher display name: `Orhan Gökçöl`

These public values are configured as GitHub Actions variables. The product's
reserved display name is configured separately in `MSIX_DISPLAY_NAME`; it must
match Partner Center exactly, including punctuation.

Public app version is **2.1.0**; the independent MSIX version is **2.1.0.0**.
This preserves increasing package order after the earlier 1.x packages.
Increase `version.txt` for subsequent Store releases. Revision (fourth field)
must remain zero; each component must fit 16 bits.

The script requires the Windows SDK and produces an x64 package for Windows 10
2004 (19041) or later, including Windows 11. It uses MakePri and MakeAppx with
semantic validation, then verifies executable, identity and display-name
round trips, SHA-256 and source commit metadata. The workflow tests rejected
identities/versions and XML escaping. On a disposable Windows runner it signs
only a temporary copy with a one-day test certificate, checks install/uninstall
and existing-data preservation, then removes that certificate and signed copy.
These are automated packaging checks, not interactive app certification.

## Capabilities and data

- `runFullTrust`: required for native desktop capture, file dialogs, GPU editing,
  clipboard and tray/keyboard integration at the user's privilege level.
  MyImage does not request administrator elevation.
- `unvirtualizedResources`: preserves preferences, custom stamps and unfinished
  project recovery data, sharing the existing portable app's data paths.
  Windows 11 exclusions cover only `LocalAppData\MyImage\MyImage` and
  `RoamingAppData\MyImage\MyImage`; Windows 10 needs the broader file-system opt-out.
  Registry virtualization remains enabled.

Captures/projects stay in Pictures/MyImage or the user-chosen folder. The Store
startup task is declared disabled; users can opt in through Settings to start
MyImage at login. No default-app change, background service or file association
is registered.
The Store package opens files using MyImage's Open controls. Microsoft must
review the restricted capability explanations.

## Submission checklist

1. Supply the real identity and build from the final release tag.
2. Test the packaged app on Windows, including upgrades, uninstall, storage,
   capture permissions, multiple displays, tray, shortcuts and recovery.
3. Run Windows App Certification Kit on that exact package and review its report.
4. Complete the Store listing with accurate Windows screenshots, description,
   age ratings, support, privacy policy, pricing and markets.
5. Upload the real-identity MSIX to Partner Center and submit for certification.
   Microsoft signing and a live Store download are confirmed only afterwards.

Privacy policy: https://github.com/gokcol/MyImage/blob/main/PRIVACY.md

References:
[MSIX signing](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview),
[package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements),
[flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization),
[Store policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies).
