//! Cameras and memory cards: mounted volumes with a `DCIM` folder (the DCF layout every camera
//! writes). Listed by `library.devices`; the app offers them under File → Add from Device, and
//! the import review copies from them into the library.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Device {
    /// The volume's name ("EOS_DIGITAL", "Untitled").
    pub name: String,
    /// Its `DCIM` folder (what gets imported).
    pub path: String,
    /// The volume's mount point.
    pub root: String,
}

/// Folders whose children are mounted volumes on this platform. `LIGHTCRAFT_DEVICE_ROOTS`
/// (paths joined like `PATH`) replaces them (tests, unusual mounts).
fn mount_parents() -> Vec<std::path::PathBuf> {
    if let Some(v) = std::env::var_os("LIGHTCRAFT_DEVICE_ROOTS") {
        return std::env::split_paths(&v).collect();
    }
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.push("/Volumes".into());
    } else if cfg!(target_os = "linux") {
        if let Ok(user) = std::env::var("USER") {
            out.push(format!("/media/{user}").into());
            out.push(format!("/run/media/{user}").into());
        }
        out.push("/media".into());
        out.push("/mnt".into());
    }
    out
}

/// Mounted volumes that look like a camera or a memory card. Cached for a couple of seconds:
/// menus ask often, and listing mounts can be slow (network volumes).
pub fn devices() -> Vec<Device> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CACHE: Mutex<Option<(Instant, Vec<Device>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = c.as_ref()
        && t.elapsed() < Duration::from_secs(2)
    {
        return d.clone();
    }
    let d = scan();
    *c = Some((Instant::now(), d.clone()));
    d
}

fn scan() -> Vec<Device> {
    let mut extra = Vec::new();
    if cfg!(windows) && std::env::var_os("LIGHTCRAFT_DEVICE_ROOTS").is_none() {
        extra.extend((b'D'..=b'Z').map(|c| std::path::PathBuf::from(format!("{}:\\", c as char))));
    }
    devices_in(&mount_parents(), extra)
}

/// Devices among the children of `parents`, plus the volume roots `extra`.
pub fn devices_in(parents: &[std::path::PathBuf], extra: Vec<std::path::PathBuf>) -> Vec<Device> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for parent in parents {
        if let Ok(rd) = std::fs::read_dir(parent) {
            let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            v.sort();
            // the startup disk shows up in /Volumes as a symlink to /
            roots.extend(v.into_iter().filter(|p| !p.is_symlink()));
        }
    }
    roots.extend(extra);
    let mut out: Vec<Device> = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let Some(dcim) = rd.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("DCIM"))) else {
            continue;
        };
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.to_string_lossy().to_string());
        if !out.iter().any(|d| d.root == root.to_string_lossy()) {
            out.push(Device { name, path: dcim.to_string_lossy().to_string(), root: root.to_string_lossy().to_string() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_with_a_dcim_folder_are_devices() {
        let base = std::env::temp_dir().join(format!("lc-devices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("EOS_DIGITAL/DCIM/100CANON")).unwrap();
        std::fs::create_dir_all(base.join("Backup/Photos")).unwrap();
        std::fs::create_dir_all(base.join("SD/dcim")).unwrap();
        let d = devices_in(std::slice::from_ref(&base), Vec::new());
        let names: Vec<&str> = d.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["EOS_DIGITAL", "SD"], "{d:?}");
        assert!(d[0].path.ends_with("DCIM") && d[1].path.ends_with("dcim"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
