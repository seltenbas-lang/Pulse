//! Settings > About > Updates: new versions are published as releases of the app's GitHub
//! repository. Pulse looks at the latest one (quietly once a day, or on "Check now"), and when it
//! is newer, downloads its APK and opens Android's installer.

use super::*;

/// Where releases are published: the latest full release, or (with "Get beta versions" on) the
/// newest of all of them, pre-releases included.
pub const RELEASES: &str = "https://api.github.com/repos/seltenbas-lang/Pulse/releases/latest";
pub const ALL_RELEASES: &str = "https://api.github.com/repos/seltenbas-lang/Pulse/releases?per_page=20";
const DAY_MS: f64 = 24.0 * 3600.0 * 1000.0;

#[derive(Default)]
pub(super) struct State {
    /// what the Updates row says
    pub status: String,
    /// a newer version and its APK
    pub found: Option<(String, String, u64)>,
    pub busy: bool,
}

/// "v2.0.10" / "2.0.10" -> (2, 0, 10); anything unreadable is (0, 0, 0).
pub(super) fn version_of(s: &str) -> (u32, u32, u32) {
    let mut n = s.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map(|p| p.parse().unwrap_or(0));
    (n.next().unwrap_or(0), n.next().unwrap_or(0), n.next().unwrap_or(0))
}

fn apk_of(release: &Value) -> Option<(String, String, u64)> {
    let version = js::str_or_empty(release.get("tag_name"));
    let apk = js::list(release.get("assets"))
        .iter()
        .find(|a| js::str_or_empty(a.get("name")).ends_with(".apk"))
        .cloned()?;
    Some((
        version.trim_start_matches(['v', 'V']).to_string(),
        js::str_or_empty(apk.get("browser_download_url")),
        apk.get("size").and_then(Value::as_u64).unwrap_or(0),
    ))
}

/// The newest release to offer: (version, APK url, APK size, beta?), or why not.
async fn latest(net: &Net, beta: bool) -> Result<(String, String, u64, bool), String> {
    let r = net
        .client
        .get(if beta { ALL_RELEASES } else { RELEASES })
        .header("User-Agent", "Pulse-Android")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    match r.status().as_u16() {
        200 => {}
        404 => return Err("no releases can be seen (the repository may be private)".into()),
        s => return Err(format!("GitHub answered {s}")),
    }
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    if !beta {
        let (ver, url, size) = apk_of(&v).ok_or("the latest release has no APK")?;
        return Ok((ver, url, size, false));
    }
    js::list(Some(&v))
        .iter()
        .filter(|r| !r.get("draft").is_some_and(js::truthy))
        .filter_map(|r| apk_of(r).map(|(ver, url, size)| (ver, url, size, r.get("prerelease").is_some_and(js::truthy))))
        .max_by_key(|(ver, ..)| version_of(ver))
        .ok_or_else(|| "no release has an APK".into())
}

impl App {
    /// Looks for a newer version. `manual`: the person pressed "Check now" (otherwise at most once
    /// a day, and quietly unless there is one).
    pub(super) fn check_updates(&mut self, manual: bool) {
        let last = self.store.number("pulse_update_last", 0.0);
        if !manual && now_ms() - last < DAY_MS {
            return;
        }
        if self.update.busy {
            return;
        }
        self.update.busy = true;
        self.update.status = "Checking…".into();
        self.show_settings_groups();
        let net = self.net.clone();
        let beta = self.store.get("pulse_update_beta") == Some("1");
        self.rt.spawn(async move {
            let r = latest(&net, beta).await;
            later(move |a| {
                a.update.busy = false;
                a.store.set("pulse_update_last", js::num_to_string(now_ms().round()));
                match r {
                    Ok((v, url, size, pre)) if version_of(&v) > version_of(APP_VERSION) && !url.is_empty() => {
                        let v = if pre { format!("{v} (beta)") } else { v };
                        a.update.status = format!("Pulse {v} is available");
                        a.store.set("pulse_update_found", v.clone());
                        journal(&format!("update: {v} available"));
                        a.toast(&format!("Pulse {v} is available: Settings → About → Update"));
                        a.update.found = Some((v, url, size));
                    }
                    Ok(_) => {
                        a.update.status = format!("You have the latest version ({APP_VERSION})");
                        a.update.found = None;
                        a.store.remove("pulse_update_found");
                        if manual {
                            a.toast("Pulse is up to date");
                        }
                    }
                    Err(e) => {
                        a.update.status = format!("Couldn't check: {e}");
                        journal(&format!("update check: {e}"));
                        if manual {
                            a.toast(&format!("Couldn't check for updates: {e}"));
                        }
                    }
                }
                a.persist();
                a.show_settings_groups();
            });
        });
    }

    /// Downloads the new version's APK into files/updates and opens the installer.
    pub(super) fn start_update(&mut self) {
        let Some((version, url, size)) = self.update.found.clone() else {
            self.check_updates(true);
            return;
        };
        if self.update.busy {
            return;
        }
        self.update.busy = true;
        self.update.status = format!("Downloading Pulse {version}…");
        self.show_settings_groups();
        let dir = self.platform.data_dir.join("updates");
        let net = self.net.clone();
        self.rt.spawn(async move {
            let file_version = version.split(' ').next().unwrap_or(&version).to_string();
            let path = dir.join(format!("pulse-{file_version}.apk"));
            let r: Result<(), String> = async {
                // only this one: older downloads go
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let mut resp = net
                    .client
                    .get(&url)
                    .header("User-Agent", "Pulse-Android")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !resp.status().is_success() {
                    return Err(format!("download failed: {}", resp.status().as_u16()));
                }
                use std::io::Write;
                let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                let (mut got, mut shown) = (0u64, 0u64);
                while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
                    f.write_all(&chunk).map_err(|e| e.to_string())?;
                    got += chunk.len() as u64;
                    if size > 0 && got * 10 / size > shown {
                        shown = got * 10 / size;
                        let pct = shown * 10;
                        later(move |a| {
                            a.update.status = format!("Downloading… {pct}%");
                            a.show_settings_groups();
                        });
                    }
                }
                if size > 0 && got != size {
                    return Err(format!("the download was incomplete ({got} of {size} bytes)"));
                }
                Ok(())
            }
            .await;
            later(move |a| {
                a.update.busy = false;
                match r {
                    Ok(()) => {
                        journal(&format!("update: {version} downloaded, opening the installer"));
                        match (a.platform.install_apk)(&path).as_str() {
                            "ok" => a.update.status = format!("Install Pulse {version} in the window that opened"),
                            "permission" => {
                                a.update.status = "Allow \"Install unknown apps\" for Pulse, then come back and press Update".into();
                                a.toast("Allow Pulse to install apps, then press Update again");
                            }
                            other => {
                                a.update.status = format!("Couldn't open the installer: {other}");
                                a.toast(&a.update.status.clone());
                            }
                        }
                    }
                    Err(e) => {
                        a.update.status = format!("Couldn't download the update: {e}");
                        a.toast(&a.update.status.clone());
                        journal(&format!("update download: {e}"));
                    }
                }
                a.show_settings_groups();
            });
        });
    }

    /// The Updates row in Settings > About.
    pub(super) fn updates_row(&self) -> (String, String) {
        let desc = if self.update.status.is_empty() {
            match self.store.get("pulse_update_found") {
                Some(v) if version_of(v) > version_of(APP_VERSION) => format!("Pulse {v} is available"),
                _ => format!("Version {APP_VERSION}. New versions come from the Pulse releases on GitHub"),
            }
        } else {
            self.update.status.clone()
        };
        let button = match &self.update.found {
            Some((v, ..)) => format!("Update to {v}"),
            None => "Check now".into(),
        };
        (desc, button)
    }
}

#[cfg(test)]
mod tests {
    use super::version_of;

    #[test]
    fn versions_compare_by_number() {
        assert!(version_of("v2.0.10") > version_of("2.0.9"));
        assert!(version_of("2.1.0") > version_of("2.0.10"));
        assert_eq!(version_of("v2.0.5"), (2, 0, 5));
        assert_eq!(version_of("Release"), (0, 0, 0));
    }
}
