use super::Outcome;
use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u32, u32, u32, u32);

impl Version {
    fn parse(tag: &str) -> Option<Self> {
        let (version, build) = tag.strip_prefix('v')?.split_once("-build")?;
        let parts: Vec<_> = version.split('.').collect();
        if parts.len() != 3
            || parts
                .iter()
                .chain(std::iter::once(&build))
                .any(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        Some(Self(
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
            build.parse().ok()?,
        ))
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    state: String,
    size: u64,
}

fn evaluate(json: &str, installed: &str, architecture: &str) -> Result<Outcome, String> {
    let release: Release = serde_json::from_str(json)
        .map_err(|_| "GitHub returned an unreadable release. Try again later.")?;
    let latest = Version::parse(&release.tag_name)
        .ok_or("The latest release has an unrecognized version.")?;
    let installed = Version::parse(installed).ok_or("This build has an unrecognized version.")?;
    if release.draft || release.prerelease {
        return Err("No stable release was returned by GitHub.".into());
    }
    if latest <= installed {
        return Ok(Outcome::Current);
    }
    let arch = match architecture {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => return Err("No Mac download is available for this processor.".into()),
    };
    let filename = format!("MyImage-{}-macos-{arch}.dmg", &release.tag_name[1..]);
    let url = format!(
        "https://github.com/gokcol/MyImage/releases/download/{}/{filename}",
        release.tag_name
    );
    // Remote text is plain text; only an exact repository/asset URL can become an action.
    if !release.assets.iter().any(|a| {
        a.name == filename && a.browser_download_url == url && a.state == "uploaded" && a.size > 0
    }) {
        return Err("The latest release does not yet include a compatible Mac download. Check the release page later.".into());
    }
    Ok(Outcome::Available {
        id: release.tag_name.clone(),
        title: format!(
            "MyImage {} · build {}",
            release.tag_name[1..].split_once("-build").unwrap().0,
            latest.3
        ),
        notes: release
            .body
            .unwrap_or_else(|| "See the release page for details.".into())
            .chars()
            .take(12_000)
            .collect(),
        url,
    })
}

#[cfg(target_os = "macos")]
pub(super) fn check() -> Result<Outcome, String> {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(std::time::Duration::from_secs(20)))
            .build(),
    );
    let response = agent
        .get("https://api.github.com/repos/gokcol/MyImage/releases/latest")
        .header("User-Agent", "MyImage-update-check")
        .header("Accept", "application/vnd.github+json")
        .call();
    let mut response = match response {
        Ok(value) => value,
        Err(ureq::Error::StatusCode(403 | 429)) => {
            return Err(
                "GitHub is limiting update checks. Try again later or open the release page."
                    .into(),
            );
        }
        Err(_) => return Err("Couldn't reach GitHub. Check your connection and try again.".into()),
    };
    let json = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_string()
        .map_err(|_| "Couldn't read the GitHub response. Try again later.")?;
    evaluate(&json, &super::release_id(), std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> serde_json::Value {
        let name = format!("MyImage-{}-macos-arm64.dmg", &tag[1..]);
        serde_json::json!({"tag_name":tag, "draft":false, "prerelease":false, "body":"Useful changes", "assets":[{"name":name,"browser_download_url":format!("https://github.com/gokcol/MyImage/releases/download/{tag}/{name}"),"state":"uploaded","size":123}]})
    }
    #[test]
    fn numeric_versions_include_build_and_reject_bad_tags() {
        assert!(Version::parse("v2.2.1-build100") > Version::parse("v2.2.1-build99"));
        assert!(Version::parse("v2.10.0-build1") > Version::parse("v2.9.9-build999"));
        for invalid in [
            "v2.2-build1",
            "v2.2.1-rc1-build101",
            "v2.2.1-build1/evil",
            "v2.2.1-build+1",
        ] {
            assert!(Version::parse(invalid).is_none());
        }
    }
    #[test]
    fn stable_release_must_have_exact_compatible_repository_asset() {
        let mut r = release("v2.2.2-build101");
        assert!(matches!(
            evaluate(&r.to_string(), "v2.2.1-build100", "aarch64"),
            Ok(Outcome::Available { .. })
        ));
        assert!(matches!(
            evaluate(&r.to_string(), "v2.2.2-build101", "aarch64"),
            Ok(Outcome::Current)
        ));
        assert!(matches!(
            evaluate(&r.to_string(), "v2.2.3-build102", "aarch64"),
            Ok(Outcome::Current)
        ));
        assert!(evaluate(&r.to_string(), "v2.2.1-build100", "x86_64").is_err());
        r["prerelease"] = true.into();
        assert!(evaluate(&r.to_string(), "v2.2.1-build100", "aarch64").is_err());
        r["prerelease"] = false.into();
        r["assets"][0]["browser_download_url"] = "https://example.com/app.dmg".into();
        assert!(evaluate(&r.to_string(), "v2.2.1-build100", "aarch64").is_err());
        r["assets"] = serde_json::json!([]);
        assert!(evaluate(&r.to_string(), "v2.2.1-build100", "aarch64").is_err());
        assert!(evaluate("not JSON", "v2.2.1-build100", "aarch64").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Contacts the public GitHub release API; run explicitly with network access"]
    fn live_github_release_check() {
        let outcome = check().expect("public GitHub release API is reachable");
        assert!(matches!(
            outcome,
            Outcome::Current | Outcome::Available { .. }
        ));
    }
}
