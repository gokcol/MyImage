//! Explicit, one-time local identity setup. Never called by builds or the app.
use crate::{StagingDirectory, TaskResult, checked};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    path::PathBuf,
    process::{Command, Stdio},
};

const NAME: &str = "MyImage Local Development";

pub fn setup(workspace: &Path) -> TaskResult<()> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?);
    let config = home.join("Library/Application Support/MyImage");
    let identity_file = config.join("signing-identity.txt");
    if identity_file.try_exists()? {
        return Err(
            "A MyImage signing identity is already configured; keep it for future builds".into(),
        );
    }
    let existing = Command::new("/usr/bin/security")
        .args(["find-certificate", "-c", NAME])
        .output()?;
    if existing.status.success() {
        return Err("A MyImage Local Development certificate already exists. Configure its fingerprint instead of creating a replacement".into());
    }
    if existing.status.code() != Some(44) {
        return Err(
            "Could not check for an existing certificate; no new identity was created".into(),
        );
    }
    let binary = workspace.join("target/release/myimage");
    if !binary.is_file() {
        return Err(
            "Build MyImage with cargo build --release before setting up local signing".into(),
        );
    }
    let keychain = Command::new("/usr/bin/security")
        .args(["default-keychain", "-d", "user"])
        .output()?;
    if !keychain.status.success() {
        return Err("Could not locate the user keychain".into());
    }
    let keychain = String::from_utf8(keychain.stdout)?;
    let keychain = keychain.trim().trim_matches('"');
    if keychain.is_empty() || !Path::new(keychain).is_file() {
        return Err("The user keychain is unavailable".into());
    }

    // Private material is temporary, beneath a mode-0700 directory. Only the
    // public certificate/fingerprint survives on disk; the key stays in Keychain.
    let temporary = StagingDirectory::create(&std::env::temp_dir())?;
    let key = temporary.path.join("private.pem");
    let certificate = temporary.path.join("certificate.pem");
    let package = temporary.path.join("identity.p12");
    checked(
        Command::new("/usr/bin/openssl")
            .args([
                "req", "-x509", "-newkey", "rsa:3072", "-nodes", "-sha256", "-days", "3650",
                "-subj",
            ])
            .arg(format!("/CN={NAME}/"))
            .args([
                "-addext",
                "basicConstraints=critical,CA:FALSE",
                "-addext",
                "keyUsage=critical,digitalSignature",
                "-addext",
                "extendedKeyUsage=critical,codeSigning",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&certificate),
        "Create a dedicated local code-signing certificate",
    )?;
    // macOS's PKCS#12 importer can reject an empty wrapping password. Generate
    // a disposable random one, never persist or print it, and pass it to OpenSSL
    // through stdin. It protects only this short-lived transfer into Keychain.
    let mut random = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let transfer_password: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let mut export = Command::new("/usr/bin/openssl")
        .args(["pkcs12", "-export", "-inkey"])
        .arg(&key)
        .arg("-in")
        .arg(&certificate)
        .args(["-name", NAME, "-passout", "stdin", "-out"])
        .arg(&package)
        .stdin(Stdio::piped())
        .spawn()?;
    writeln!(
        export.stdin.take().ok_or("Missing OpenSSL input")?,
        "{transfer_password}"
    )?;
    if !export.wait()?.success() {
        return Err("Could not prepare the temporary Keychain import".into());
    }
    let fingerprint = Command::new("/usr/bin/openssl")
        .args(["x509", "-noout", "-fingerprint", "-sha1", "-in"])
        .arg(&certificate)
        .output()?;
    if !fingerprint.status.success() {
        return Err("Could not read the certificate fingerprint".into());
    }
    let fingerprint = String::from_utf8(fingerprint.stdout)?;
    let fingerprint = fingerprint
        .trim()
        .split_once('=')
        .ok_or("Invalid certificate fingerprint output")?
        .1
        .replace(':', "");
    if fingerprint.len() != 40 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid certificate fingerprint".into());
    }
    checked(
        Command::new("/usr/bin/security")
            .arg("import")
            .arg(&package)
            .args([
                "-k",
                keychain,
                "-P",
                &transfer_password,
                "-x",
                "-T",
                "/usr/bin/codesign",
            ]),
        "Import local identity into your Keychain (codesign access only, non-exportable key)",
    )?;
    // No trust settings or security policies are changed. This certificate is
    // only an identity for this Mac; it is not an Apple distribution identity.
    let probe = temporary.path.join("SigningProbe.app/Contents");
    fs::create_dir_all(probe.join("MacOS"))?;
    fs::copy(&binary, probe.join("MacOS/MyImage"))?;
    fs::copy(
        workspace.join("packaging/macos/Info.plist"),
        probe.join("Info.plist"),
    )?;
    let probe_app = probe.parent().ok_or("Missing signing probe directory")?;
    if let Err(error) = checked(
        Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", &fingerprint])
            .arg(probe_app),
        "Verify that the new identity can sign MyImage",
    ) {
        return Err(format!("{error}. The new certificate remains in Keychain with fingerprint {fingerprint}; no build identity was configured. Do not create another certificate.").into());
    }
    checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "--deep"])
            .arg(probe_app),
        "Verify the certificate-signed probe",
    )?;
    fs::create_dir_all(&config)?;
    fs::copy(&certificate, config.join("local-signing-certificate.pem"))?;
    // create_new avoids accidentally replacing an identity configured meanwhile.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&identity_file)?;
    writeln!(file, "{fingerprint}")?;
    println!(
        "Configured local signer {fingerprint} at {}",
        identity_file.display()
    );
    println!(
        "Keep this identity for all future MyImage updates. No Screen Recording grants were changed."
    );
    Ok(())
}
