//! Check GitHub Releases for a newer version and, where possible, install it in place.
//!
//! Installing swaps the running executable for the new one at the same path, so
//! shortcuts and taskbar pins keep working. The new version starts on next launch.

use serde::Deserialize;
use std::io::Read;

const LATEST: &str = "https://api.github.com/repos/vhorvath2010/open-one-piano/releases/latest";
const USER_AGENT: &str = concat!("open-one-piano/", env!("CARGO_PKG_VERSION"));
/// Release downloads are a few MB; refuse anything absurd.
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

/// The version this build reports. Debug builds can pretend to be older with
/// `OPEN_ONE_PIANO_FAKE_VERSION=0.1.0` to try the update flow.
pub fn current_version() -> String {
    #[cfg(debug_assertions)]
    if let Ok(v) = std::env::var("OPEN_ONE_PIANO_FAKE_VERSION") {
        return v;
    }
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Clone, Debug)]
pub struct Release {
    /// Version without the leading "v", e.g. "0.4.0".
    pub version: String,
    /// Release page, for notes and manual download.
    pub page: String,
    /// Archive for this platform, if this platform can update in place.
    pub asset: Option<String>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// The release archive name for this platform that holds a bare executable we
/// can swap in. macOS ships an .app bundle, which we don't patch in place.
fn asset_name() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("open-one-piano-windows-x64.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("open-one-piano-linux-x64.tar.gz")
    } else {
        None
    }
}

fn bin_name() -> &'static str {
    if cfg!(windows) {
        "open-one-piano.exe"
    } else {
        "open-one-piano"
    }
}

/// "v1.10.0" -> [1, 10, 0]. Anything after a '-' or '+' is ignored.
fn parse(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse(candidate), parse(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))
}

/// Ask GitHub for the latest release. `Ok(None)` means we're up to date.
pub fn check() -> Result<Option<Release>, String> {
    let mut resp = get(LATEST)?;
    let text = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Couldn't read the update info: {e}"))?;
    let rel: ApiRelease =
        serde_json::from_str(&text).map_err(|e| format!("Unexpected update info: {e}"))?;
    if rel.draft || rel.prerelease || !is_newer(&rel.tag_name, &current_version()) {
        return Ok(None);
    }
    let asset = asset_name().and_then(|want| {
        rel.assets
            .iter()
            .find(|a| a.name == want)
            .map(|a| a.browser_download_url.clone())
    });
    Ok(Some(Release {
        version: rel.tag_name.trim_start_matches('v').to_string(),
        page: rel.html_url,
        asset,
    }))
}

/// Download the release archive and pull out the executable.
pub fn download(asset_url: &str) -> Result<Vec<u8>, String> {
    let mut resp = get(asset_url)?;
    let archive = resp
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .map_err(|e| format!("Download failed: {e}"))?;
    let exe = if asset_url.ends_with(".zip") {
        from_zip(&archive)?
    } else {
        from_tar_gz(&archive)?
    };
    if exe.is_empty() {
        return Err("The download didn't contain the app.".into());
    }
    Ok(exe)
}

fn from_zip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("Couldn't open the download: {e}"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map_err(|e| e.to_string())?.to_string();
        if file_name(&name) == bin_name() {
            let mut out = Vec::new();
            f.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err("The download didn't contain the app.".into())
}

fn from_tar_gz(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut e = entry.map_err(|e| e.to_string())?;
        let name = e
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        if file_name(&name) == bin_name() {
            let mut out = Vec::new();
            e.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err("The download didn't contain the app.".into())
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Replace the running executable with `exe`. Takes effect on next launch.
pub fn install(exe: &[u8]) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("open-one-piano-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(bin_name());
    std::fs::write(&tmp, exe).map_err(|e| format!("Couldn't save the update: {e}"))?;
    let result = self_replace::self_replace(&tmp)
        .map_err(|e| format!("Couldn't replace the app (is its folder writable?): {e}"));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.4.0", "0.3.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.99.0"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("v0.3", "0.3.0"));
        assert!(!is_newer("v0.2.9", "0.3.0"));
        assert!(!is_newer("v0.3.0-beta", "0.3.0"));
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("dist/open-one-piano.exe"), "open-one-piano.exe");
        assert_eq!(file_name("./open-one-piano"), "open-one-piano");
        assert_eq!(file_name("open-one-piano"), "open-one-piano");
    }

    /// Hits the network: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_latest_release() {
        let rel = check_against("0.0.1").expect("check").expect("a release");
        let exe = download(&rel.asset.expect("asset for this platform")).expect("download");
        assert!(exe.len() > 100_000, "exe is {} bytes", exe.len());
    }

    /// Replaces the running test binary, so only run it on a throwaway copy:
    /// copy the test exe elsewhere and run `<copy> installs_over_itself --ignored`.
    #[test]
    #[ignore]
    fn installs_over_itself() {
        let rel = check_against("0.0.1").expect("check").expect("a release");
        let exe = download(&rel.asset.expect("asset")).expect("download");
        install(&exe).expect("install");
        let now = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        assert!(now == exe, "exe on disk wasn't replaced");
    }

    fn check_against(v: &str) -> Result<Option<Release>, String> {
        // SAFETY: test-only; no other test in this module reads env vars concurrently.
        unsafe { std::env::set_var("OPEN_ONE_PIANO_FAKE_VERSION", v) };
        check()
    }
}
