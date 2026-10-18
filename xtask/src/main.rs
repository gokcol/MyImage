use std::{
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

type TaskResult<T> = Result<T, Box<dyn Error>>;

mod cleanup;

#[cfg(target_os = "macos")]
mod install;
#[cfg(target_os = "macos")]
mod local_signing;

const USAGE: &str = "macOS update: cargo run -p xtask -- install
Notarize using an existing Keychain profile: cargo run -p xtask -- notarize PROFILE
Usage: cargo run -p xtask -- bundle [--release | --debug] [--ad-hoc]\n\
    Release is the default. macOS requires a persistent signing identity unless --ad-hoc is explicit.\n\
    Successful release bundles retain one rollback per platform and clean build caches/old QA apps.\n\
    Regenerate the calico icon assets on macOS: cargo run -p xtask -- icons\n\
    Set up a local signing certificate (explicit opt-in): cargo run -p xtask -- setup-local-signing --confirm";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Packaging failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> TaskResult<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if matches!(command.as_deref(), Some("--help" | "-h") | None) {
        println!("{USAGE}");
        return Ok(());
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Could not locate the workspace")?;
    #[cfg(target_os = "macos")]
    if command.as_deref() == Some("setup-local-signing") {
        if args.next().as_deref() != Some("--confirm") || args.next().is_some() {
            return Err("This creates a dedicated certificate/private key in your login Keychain and configures MyImage builds to use it. Read packaging/macos/SIGNING.md, then pass --confirm to opt in.".into());
        }
        return local_signing::setup(workspace);
    }
    #[cfg(target_os = "macos")]
    if command.as_deref() == Some("install") {
        let mut migration = false;
        let mut unnotarized = false;
        for arg in args {
            match arg.as_str() {
                "--allow-signing-migration" => migration = true,
                "--allow-unnotarized" => unnotarized = true,
                _ => return Err(format!("Unknown install option: {arg}").into()),
            }
        }
        return install::run(workspace, migration, unnotarized);
    }
    #[cfg(target_os = "macos")]
    if command.as_deref() == Some("notarize") {
        let profile = args
            .next()
            .ok_or("Supply the name of an existing notarytool Keychain profile.")?;
        if args.next().is_some() {
            return Err("Unexpected notarize argument".into());
        }
        return install::notarize(workspace, &profile);
    }
    if command.as_deref() == Some("icons") {
        if args.next().is_some() {
            return Err(format!("Unexpected icon option. {USAGE}").into());
        }
        return generate_icons(workspace);
    }
    if command.as_deref() != Some("bundle") {
        return Err(format!("Unknown command. {USAGE}").into());
    }
    let mut profile_option = None;
    let mut ad_hoc = false;
    for argument in args {
        match argument.as_str() {
            "--release" if profile_option.is_none() => profile_option = Some(true),
            "--debug" if profile_option.is_none() => profile_option = Some(false),
            "--ad-hoc" if !ad_hoc => ad_hoc = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("Unknown or repeated option: {argument}. {USAGE}").into()),
        }
    }

    let release = profile_option.unwrap_or(true);
    if !cfg!(any(target_os = "macos", target_os = "windows")) {
        return Err("Bundle on macOS or Windows; this helper does not cross-package apps".into());
    }
    if std::env::var_os("CARGO_BUILD_TARGET").is_some() {
        return Err("Unset CARGO_BUILD_TARGET to bundle for this machine's native platform".into());
    }
    // Resolve the signer before spending time on a build. Never silently fall
    // back to an ad-hoc signature: that changes macOS's privacy identity.
    #[cfg(target_os = "macos")]
    let signing_identity = macos_signing_identity(ad_hoc)?;
    #[cfg(not(target_os = "macos"))]
    if ad_hoc {
        return Err("--ad-hoc is a macOS-only development option".into());
    }
    let target = workspace.join("target");
    if release {
        cleanup::check_workspace(workspace)?;
    }
    let _release_lock = release
        .then(|| cleanup::release_lock(workspace))
        .transpose()?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut build = Command::new(cargo);
    build
        .current_dir(workspace)
        .args([
            "build",
            "--locked",
            "--package",
            "myimage",
            "--bin",
            "myimage",
        ])
        .arg("--target-dir")
        .arg(&target);
    if release {
        build.arg("--release");
    }
    // Match the bundle's declared minimum OS instead of inheriting the SDK's
    // newer default. This affects this build process only.
    if cfg!(target_os = "macos") {
        build.env("MACOSX_DEPLOYMENT_TARGET", "14.0");
    }
    checked(&mut build, "Build MyImage")?;

    let profile = if release { "release" } else { "debug" };
    let binary_dir = target.join(profile);
    let executable = binary_dir.join(if cfg!(target_os = "windows") {
        "myimage.exe"
    } else {
        "myimage"
    });
    if !executable.is_file() {
        return Err(format!(
            "Build succeeded but {} is missing. Bundle a native build without a custom \
             Cargo build.target configuration.",
            executable.display()
        )
        .into());
    }

    let bundle_dir = if release {
        target.join("bundle")
    } else {
        target.join("bundle/debug")
    };
    fs::create_dir_all(&bundle_dir)?;
    #[cfg(target_os = "macos")]
    bundle(
        workspace,
        &executable,
        &bundle_dir,
        &signing_identity,
        release,
    )?;
    #[cfg(not(target_os = "macos"))]
    bundle(workspace, &executable, &bundle_dir, release)?;
    Ok(())
}

fn checked(command: &mut Command, description: &str) -> TaskResult<()> {
    println!("{description}");
    let status = command
        .status()
        .map_err(|error| format!("{description} could not start: {error}"))?;
    if !status.success() {
        return Err(format!("{description} failed with {status}").into());
    }
    Ok(())
}

/// Build in a temporary sibling directory. Failed signing or archiving leaves
/// the previous successful bundle intact and is reported as a failed command.
struct StagingDirectory {
    path: PathBuf,
    remove_on_drop: bool,
}

impl StagingDirectory {
    fn create(parent: &Path) -> TaskResult<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = parent.join(format!(".myimage-{}-{timestamp}", std::process::id()));
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        builder.create(&path)?;
        Ok(Self {
            path,
            remove_on_drop: true,
        })
    }

    fn publish(&mut self, source: &Path, destination: &Path, backup_name: &str) -> TaskResult<()> {
        let backup = self.path.join(backup_name);
        if let Err(error) = replace_artifact(source, destination, &backup) {
            // In particular, never delete the old artifact if restoring a
            // failed replacement also failed (for example, a file is locked).
            self.remove_on_drop = false;
            return Err(format!(
                "{error}. Build files have been preserved at {}",
                self.path.display()
            )
            .into());
        }
        Ok(())
    }

    fn finish_release(
        &mut self,
        workspace: &Path,
        kind: cleanup::ReleaseKind,
        previous_name: &str,
        same_release: bool,
    ) -> TaskResult<()> {
        if let Err(error) = cleanup::after_release(
            workspace,
            kind,
            &self.path.join(previous_name),
            same_release,
        ) {
            // Never discard the previous release if creating its durable
            // rollback failed (e.g. full disk, permissions, locked archive).
            self.remove_on_drop = false;
            return Err(format!(
                "{error}. Previous release files remain at {}",
                self.path.display()
            )
            .into());
        }
        Ok(())
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn replace_artifact(source: &Path, destination: &Path, backup: &Path) -> TaskResult<()> {
    let had_previous = destination.try_exists()?;
    if had_previous {
        fs::rename(destination, backup)?;
    }
    if let Err(error) = fs::rename(source, destination) {
        if had_previous && let Err(restore_error) = fs::rename(backup, destination) {
            return Err(format!(
                "Could not publish {}: {error}; the previous artifact is at {} \
                 because restoring it failed: {restore_error}",
                destination.display(),
                backup.display()
            )
            .into());
        }
        return Err(error.into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_signing_identity(ad_hoc: bool) -> TaskResult<String> {
    if ad_hoc {
        eprintln!("Warning: ad-hoc development builds may require a new Screen Recording grant.");
        return Ok("-".into());
    }
    let configured = if let Ok(identity) = std::env::var("MYIMAGE_SIGNING_IDENTITY") {
        identity
    } else {
        let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
        let path =
            PathBuf::from(home).join("Library/Application Support/MyImage/signing-identity.txt");
        fs::read_to_string(&path).map_err(|_| {
            "No persistent MyImage signing identity configured. Set MYIMAGE_SIGNING_IDENTITY \
             to a certificate fingerprint, or follow packaging/macos/SIGNING.md. \
             Only use --ad-hoc for disposable development builds."
        })?
    };
    let identity = configured.trim();
    if identity.len() != 40 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Signing identity must be a 40-character certificate SHA-1 fingerprint; use --ad-hoc explicitly for unsigned development".into());
    }
    Ok(identity.to_owned())
}

#[cfg(target_os = "macos")]
fn bundle(
    workspace: &Path,
    executable: &Path,
    destination: &Path,
    signing_identity: &str,
    release: bool,
) -> TaskResult<()> {
    let mut stage = StagingDirectory::create(destination)?;
    let app = stage.path.join("MyImage.app");
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)?;
    fs::create_dir(contents.join("Resources"))?;
    fs::copy(
        workspace.join("assets/branding/MyImage.icns"),
        contents.join("Resources/MyImage.icns"),
    )?;
    fs::copy(
        workspace.join("assets/stamps/LICENSE-GRAPHICS"),
        contents.join("Resources/Twemoji-LICENSE.txt"),
    )?;
    fs::copy(
        workspace.join("assets/stamps/ATTRIBUTION.txt"),
        contents.join("Resources/Twemoji-ATTRIBUTION.txt"),
    )?;
    fs::copy(executable, macos.join("MyImage"))?;
    fs::copy(
        workspace.join("packaging/macos/Info.plist"),
        contents.join("Info.plist"),
    )?;
    checked(
        Command::new("/usr/bin/plutil")
            .arg("-lint")
            .arg(contents.join("Info.plist")),
        "Validate app metadata",
    )?;
    let mut sign = Command::new("/usr/bin/codesign");
    sign.args(["--force", "--sign", signing_identity]);
    if std::env::var_os("MYIMAGE_DISTRIBUTION").is_some() {
        if signing_identity == "-" {
            return Err("Distribution cannot use an ad-hoc signature".into());
        }
        sign.args(["--options", "runtime", "--timestamp"]);
    }
    checked(sign.arg(&app), "Sign MyImage.app")?;
    checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app),
        "Verify app signature",
    )?;

    // Uploading a bare .app through artifact services can strip executable
    // permissions. ditto preserves the complete signed bundle in this ZIP.
    let archive_name = format!("MyImage-macos-{}.zip", std::env::consts::ARCH);
    let archive = stage.path.join(&archive_name);
    checked(
        Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(&app)
            .arg(&archive),
        "Archive MyImage.app",
    )?;
    let final_app = destination.join("MyImage.app");
    // Repackaging the same numbered release must not replace a useful older
    // rollback with another copy of the current release.
    let same_release = cleanup::same_file_contents(
        &contents.join("Info.plist"),
        &final_app.join("Contents/Info.plist"),
    )?;
    stage.publish(&app, &final_app, "previous.app")?;
    let final_archive = destination.join(archive_name);
    stage.publish(&archive, &final_archive, "previous.zip")?;
    println!("Created {}", final_app.display());
    println!("Created {}", final_archive.display());
    if signing_identity == "-" {
        println!("Ad-hoc development build: Screen Recording permission may not survive updates.");
    } else {
        println!("Signed with persistent identity {signing_identity}.");
    }
    println!("Public distribution requires an Apple distribution identity and notarization.");
    if release {
        stage.finish_release(
            workspace,
            cleanup::ReleaseKind::Mac(std::env::consts::ARCH),
            "previous.zip",
            same_release,
        )?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn generate_icons(workspace: &Path) -> TaskResult<()> {
    let branding = workspace.join("assets/branding");
    let source = branding.join("myimage-calico-logo-v1.png");
    let target = workspace.join("target");
    fs::create_dir_all(&target)?;
    let stage = StagingDirectory::create(&target)?;
    let iconset = stage.path.join("MyImage.iconset");
    fs::create_dir(&iconset)?;
    for size in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let pixels = (size * scale).to_string();
            let suffix = if scale == 2 { "@2x" } else { "" };
            let output = iconset.join(format!("icon_{size}x{size}{suffix}.png"));
            checked(
                Command::new("/usr/bin/sips")
                    .args(["--resampleHeightWidth", &pixels, &pixels])
                    .arg(&source)
                    .arg("--out")
                    .arg(&output),
                "Resize calico icon representation",
            )?;
        }
    }
    checked(
        Command::new("/usr/bin/iconutil")
            .args(["--convert", "icns", "--output"])
            .arg(branding.join("MyImage.icns"))
            .arg(&iconset),
        "Create macOS app icon",
    )?;
    fs::copy(
        iconset.join("icon_256x256.png"),
        branding.join("myimage-app-icon-256.png"),
    )?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn generate_icons(_workspace: &Path) -> TaskResult<()> {
    Err("Regenerate the checked-in icon assets on macOS using sips and iconutil".into())
}

#[cfg(target_os = "windows")]
fn bundle(
    workspace: &Path,
    executable: &Path,
    destination: &Path,
    release: bool,
) -> TaskResult<()> {
    let mut stage = StagingDirectory::create(destination)?;
    for name in ["LICENSE-GRAPHICS", "ATTRIBUTION.txt"] {
        let staged = stage.path.join(name);
        fs::copy(workspace.join("assets/stamps").join(name), &staged)?;
        stage.publish(
            &staged,
            &destination.join(format!("Twemoji-{name}")),
            &format!("previous-{name}"),
        )?;
    }
    let staged_exe = stage.path.join("MyImage.exe");
    fs::copy(executable, &staged_exe)?;
    let final_exe = destination.join("MyImage.exe");
    let same_release = cleanup::same_file_contents(&staged_exe, &final_exe)?;
    stage.publish(&staged_exe, &final_exe, "previous.exe")?;
    println!("Created {}", final_exe.display());
    if release {
        stage.finish_release(
            workspace,
            cleanup::ReleaseKind::Windows(std::env::consts::ARCH),
            "previous.exe",
            same_release,
        )?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn bundle(
    _workspace: &Path,
    _executable: &Path,
    _destination: &Path,
    _release: bool,
) -> TaskResult<()> {
    Err("Bundling is supported on macOS and Windows only".into())
}

#[cfg(all(test, target_os = "macos"))]
mod packaging_tests {
    use super::*;

    /// Uses the already packaged binary; no installed app is opened or modified.
    #[test]
    #[ignore = "requires a packaged Mac app and native codesign/ps access"]
    fn signed_release_runs_cleanup_only_after_success() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        for relative in [
            "assets/branding/MyImage.icns",
            "assets/stamps/LICENSE-GRAPHICS",
            "assets/stamps/ATTRIBUTION.txt",
            "packaging/macos/Info.plist",
        ] {
            let output = root.join(relative);
            fs::create_dir_all(output.parent().unwrap()).unwrap();
            fs::copy(source.join(relative), output).unwrap();
        }
        let executable = source.join("target/bundle/MyImage.app/Contents/MacOS/MyImage");
        let destination = root.join("target/bundle");
        fs::create_dir_all(&destination).unwrap();
        let cache = root.join("target/debug/deps/old.rlib");
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(&cache, b"cache").unwrap();
        // A signing failure must retain the old caches and produce no rollback.
        assert!(
            bundle(
                root,
                &executable,
                &destination,
                "nonexistent-test-signer",
                true
            )
            .is_err()
        );
        assert!(cache.exists());
        assert!(!root.join("backups").exists());
        // Debug packaging has no cleanup side effects.
        bundle(root, &executable, &destination, "-", false).unwrap();
        assert!(cache.exists());
        let previous =
            fs::read(destination.join(format!("MyImage-macos-{}.zip", std::env::consts::ARCH)))
                .unwrap();
        // Real signing, archiving, publication, rollback retention and cleanup.
        bundle(root, &executable, &destination, "-", true).unwrap();
        assert!(!cache.exists());
        let backup = root.join("backups").join(format!(
            "MyImage-rollback-macos-{}.zip",
            std::env::consts::ARCH
        ));
        assert_eq!(fs::read(&backup).unwrap(), previous);
        assert!(
            destination
                .join("MyImage.app/Contents/MacOS/MyImage")
                .exists()
        );
        // Repeated packaging retains that rollback rather than rotating again.
        bundle(root, &executable, &destination, "-", true).unwrap();
        assert_eq!(fs::read(backup).unwrap(), previous);
        assert_eq!(fs::read_dir(root.join("backups")).unwrap().count(), 1);
    }
}
