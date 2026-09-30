#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// One installed application, as listed by the Applications page.
pub struct AppEntry {
    pub name: String,
    /// Executable, launcher or installation directory, for reference display.
    pub path: PathBuf,
    /// Resolved icon file, when the platform can find one.
    pub icon: Option<PathBuf>,
    /// Total disk size of the installation, when a manager reports it.
    pub size: Option<u64>,
    /// Last time the app was used, when the platform can tell. `None` also
    /// covers "installed but never launched".
    pub last_used: Option<SystemTime>,
    /// How to remove the app, when a safe uninstall path exists.
    pub uninstall: Option<Uninstall>,
}

/// Supported uninstall channels. The variants cover every platform; each
/// platform's scan only produces the ones it can actually execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uninstall {
    Pacman(String),
    Dpkg(String),
    Rpm(String),
    Flatpak(String),
    Snap(String),
    Windows(String),
    MacBundle(PathBuf),
}

impl Uninstall {
    /// Rebuilds an action from the `(kind, arg)` pair the desktop frontend
    /// round-trips. Unknown kinds are rejected.
    pub fn parse(kind: &str, arg: &str) -> Option<Self> {
        if arg.is_empty() {
            return None;
        }
        match kind {
            "pacman" => Some(Self::Pacman(arg.to_owned())),
            "dpkg" => Some(Self::Dpkg(arg.to_owned())),
            "rpm" => Some(Self::Rpm(arg.to_owned())),
            "flatpak" => Some(Self::Flatpak(arg.to_owned())),
            "snap" => Some(Self::Snap(arg.to_owned())),
            "windows" => Some(Self::Windows(arg.to_owned())),
            "macos" => Some(Self::MacBundle(PathBuf::from(arg))),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Pacman(_) => "pacman",
            Self::Dpkg(_) => "dpkg",
            Self::Rpm(_) => "rpm",
            Self::Flatpak(_) => "flatpak",
            Self::Snap(_) => "snap",
            Self::Windows(_) => "windows",
            Self::MacBundle(_) => "macos",
        }
    }
}

/// Lists installed applications with usage and size information. Returns an
/// empty list on platforms without an application model.
pub fn scan() -> Vec<AppEntry> {
    #[cfg(target_os = "linux")]
    {
        linux::scan()
    }
    #[cfg(windows)]
    {
        windows::scan()
    }
    #[cfg(target_os = "macos")]
    {
        macos::scan()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        Vec::new()
    }
}

/// Removes an application through its platform's uninstall channel. Native
/// package managers go through `pkexec`, which opens the desktop's polkit
/// password prompt; no password is ever handled by this app.
pub fn uninstall(action: &Uninstall) -> Result<(), String> {
    match action {
        Uninstall::Pacman(package) => {
            #[cfg(target_os = "linux")]
            {
                elevated(&["pacman", "-Rns", "--noconfirm", package])
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = package;
                Err("pacman packages can only be removed on Linux".to_owned())
            }
        }
        Uninstall::Dpkg(package) => {
            #[cfg(target_os = "linux")]
            {
                elevated(&["apt-get", "remove", "--purge", "-y", package])
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = package;
                Err("deb packages can only be removed on Linux".to_owned())
            }
        }
        Uninstall::Rpm(package) => {
            #[cfg(target_os = "linux")]
            {
                elevated(&["rpm", "-e", package])
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = package;
                Err("rpm packages can only be removed on Linux".to_owned())
            }
        }
        Uninstall::Flatpak(id) => {
            #[cfg(target_os = "linux")]
            {
                plain(&["flatpak", "uninstall", "--noninteractive", "-y", id])
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = id;
                Err("flatpaks can only be removed on Linux".to_owned())
            }
        }
        Uninstall::Snap(name) => {
            #[cfg(target_os = "linux")]
            {
                snap_remove(name)
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = name;
                Err("snaps can only be removed on Linux".to_owned())
            }
        }
        Uninstall::Windows(command_line) => {
            #[cfg(windows)]
            {
                // Uninstallers are interactive programs: launch detached and
                // let the user follow their wizard.
                std::process::Command::new("cmd")
                    .args(["/C", command_line])
                    .spawn()
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            #[cfg(not(windows))]
            {
                let _ = command_line;
                Err("Windows programs can only be uninstalled on Windows".to_owned())
            }
        }
        Uninstall::MacBundle(bundle) => {
            #[cfg(target_os = "macos")]
            {
                trash_bundle(bundle)
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = bundle;
                Err("app bundles can only be removed on macOS".to_owned())
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn elevated(command: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new("pkexec")
        .args(command)
        .status()
        .map_err(|error| format!("failed to launch pkexec: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", command[0]))
    }
}

#[cfg(target_os = "linux")]
fn plain(command: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new(command[0])
        .args(&command[1..])
        .status()
        .map_err(|error| format!("failed to launch {}: {error}", command[0]))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", command[0]))
    }
}

#[cfg(target_os = "linux")]
fn snap_remove(name: &str) -> Result<(), String> {
    // Without a running snapd the CLI hangs instead of failing, so the
    // socket is checked first.
    if !Path::new("/run/snapd.socket").exists() {
        return Err("snapd is not running".to_owned());
    }
    plain(&["snap", "remove", name])
}

#[cfg(target_os = "macos")]
fn trash_bundle(bundle: &Path) -> Result<(), String> {
    let home = crate::platform::home_dir().ok_or("home directory unavailable")?;
    let trash = home.join(".Trash");
    std::fs::create_dir_all(&trash).map_err(|error| error.to_string())?;
    let name = bundle
        .file_name()
        .ok_or("bundle has no name")?
        .to_string_lossy()
        .into_owned();
    std::fs::rename(bundle, trash.join(name)).map_err(|error| error.to_string())
}

/// Parses pacman-style sizes like "12.40 MiB".
pub fn parse_size(text: &str) -> Option<u64> {
    let (number, unit) = text.trim().split_once(' ')?;
    let value: f64 = number.parse().ok()?;
    let multiplier = match unit.trim() {
        "B" => 1.0,
        "KiB" | "kB" | "KB" => 1024.0,
        "MiB" | "MB" => 1024.0 * 1024.0,
        "GiB" | "GB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" | "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * multiplier) as u64)
}

/// ROT13, the "encryption" Windows applies to UserAssist value names.
pub fn rot13(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a'..='z' => ((c as u8 - b'a' + 13) % 26 + b'a') as char,
            'A'..='Z' => ((c as u8 - b'A' + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

/// Windows FILETIME (100ns ticks since 1601-01-01) to SystemTime.
pub fn filetime_to_system_time(ticks: u64) -> Option<SystemTime> {
    const TICKS_PER_SECOND: u64 = 10_000_000;
    const SECONDS_1601_TO_1970: u64 = 11_644_473_600;
    let unix = ticks
        .checked_div(TICKS_PER_SECOND)?
        .checked_sub(SECONDS_1601_TO_1970)?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(unix))
}

/// Extracts the last-run time from a UserAssist `Count` entry. Published
/// layouts disagree on whether the FILETIME sits at offset 64 or 68, so the
/// first offset that decodes to a plausible date wins.
pub fn last_run_from_userassist(bytes: &[u8]) -> Option<SystemTime> {
    for offset in [64_usize, 68_usize] {
        if bytes.len() < offset + 8 {
            continue;
        }
        let ticks = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
        if let Some(time) = filetime_to_system_time(ticks) {
            let sane = time
                .duration_since(SystemTime::UNIX_EPOCH)
                .is_ok_and(|age| age > Duration::from_secs(315_532_800))
                && time < SystemTime::now() + Duration::from_secs(86_400);
            if sane {
                return Some(time);
            }
        }
    }
    None
}

/// Days since 1970-01-01 for a civil (proleptic Gregorian) date.
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses Spotlight's `kMDItemLastUsedDate` output
/// ("2026-09-28 09:12:34 +0000"; "(null)" means never used).
pub fn parse_mdls_date(text: &str) -> Option<SystemTime> {
    let mut parts = text.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: i64 = time.next()?.parse().ok()?;
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    if secs <= 0 {
        return None;
    }
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
}

/// Minimal freedesktop .desktop parser: only the fields the Applications
/// page needs. Entries that are hidden or not of Type=Application are
/// rejected.
pub struct DesktopInfo {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

pub fn parse_desktop_entry(text: &str) -> Option<DesktopInfo> {
    let mut in_entry = false;
    let mut name: Option<String> = None;
    let mut localized_name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut app_type = String::new();
    let mut hidden = false;
    let mut no_display = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => app_type = value.to_owned(),
            "Name" if name.is_none() => name = Some(value.to_owned()),
            key if key.starts_with("Name[") && localized_name.is_none() => {
                localized_name = Some(value.to_owned());
            }
            "Exec" if exec.is_none() => exec = Some(value.to_owned()),
            "Icon" if icon.is_none() => icon = Some(value.to_owned()),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    let name = name.or(localized_name)?;
    let exec = exec?;
    (app_type == "Application" && !no_display && !hidden && !name.is_empty() && !exec.is_empty())
        .then_some(DesktopInfo { name, exec, icon })
}

/// What a .desktop `Exec=` line launches.
#[derive(Debug)]
pub enum ExecTarget {
    Flatpak(String),
    Snap(String),
    Binary(PathBuf),
}

pub fn classify_exec(exec: &str) -> Option<ExecTarget> {
    let mut tokens = exec.split_whitespace().peekable();
    // Skip "env" and VAR=VALUE prefixes some launchers use.
    if tokens.peek().copied() == Some("env") {
        tokens.next();
        while tokens
            .peek()
            .is_some_and(|token| token.contains('=') && !token.contains('/'))
        {
            tokens.next();
        }
    }
    let program = tokens.next()?.trim_matches('"');
    let args: Vec<&str> = tokens.collect();
    let base = program.rsplit('/').next().unwrap_or(program);
    if base == "flatpak" {
        // "flatpak run [options] <app-id> [args]": the id is the first bare
        // token after `run`.
        let run = args.iter().position(|token| *token == "run")?;
        return args
            .iter()
            .skip(run + 1)
            .find(|token| !token.starts_with('-'))
            .map(|id| ExecTarget::Flatpak((*id).to_owned()));
    }
    if base == "snap" {
        if args.first() == Some(&"run") {
            return args.get(1).map(|name| ExecTarget::Snap((*name).to_owned()));
        }
        return None;
    }
    let path = if program.contains('/') || program.contains('\\') {
        PathBuf::from(program)
    } else {
        find_in_path(program)?
    };
    path.is_file().then_some(ExecTarget::Binary(path))
}

pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    // Windows executables live behind extensions; probe the common ones.
    #[cfg(windows)]
    let names = [
        program.to_owned(),
        format!("{program}.exe"),
        format!("{program}.com"),
        format!("{program}.bat"),
    ];
    #[cfg(not(windows))]
    let names = [program.to_owned()];
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{classify_exec, parse_desktop_entry, parse_size, AppEntry, ExecTarget, Uninstall};
    use crate::parallel::parallel_map;
    use crate::platform;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::SystemTime;

    pub fn scan() -> Vec<AppEntry> {
        let desktop = collect_desktop_files();
        let pacman = pacman_sizes();
        let dpkg = dpkg_sizes();
        let rpm = rpm_sizes();
        let snap = snap_sizes();
        parallel_map(desktop, move |app| {
            let icon = resolve_desktop_icon(app.icon.as_deref());
            build_entry(app.name, app.exec, icon, &pacman, &dpkg, &rpm, &snap)
        })
        .into_iter()
        .flatten()
        .collect()
    }

    /// Desktop-file roots in priority order; user overrides win over system
    /// exports when both ship the same file name.
    fn desktop_roots() -> Vec<(u8, PathBuf)> {
        let mut roots = Vec::new();
        if let Some(home) = platform::home_dir() {
            roots.push((3, home.join(".local/share/applications")));
            roots.push((
                2,
                home.join(".local/share/flatpak/exports/share/applications"),
            ));
        }
        roots.push((
            2,
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        ));
        roots.push((2, PathBuf::from("/var/lib/snapd/desktop/applications")));
        roots.push((1, PathBuf::from("/usr/local/share/applications")));
        roots.push((1, PathBuf::from("/usr/share/applications")));
        roots
    }

    /// A desktop entry kept for the applications list.
    struct DesktopApp {
        name: String,
        exec: String,
        icon: Option<String>,
    }

    fn collect_desktop_files() -> Vec<DesktopApp> {
        let mut by_stem: HashMap<String, (u8, DesktopApp)> = HashMap::new();
        for (priority, root) in desktop_roots() {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Some(parsed) = parse_desktop_entry(&text) else {
                    continue;
                };
                let stem = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let replace = match by_stem.get(&stem) {
                    Some((existing, _)) => *existing < priority,
                    None => true,
                };
                if replace {
                    by_stem.insert(
                        stem,
                        (
                            priority,
                            DesktopApp {
                                name: parsed.name,
                                exec: parsed.exec,
                                icon: parsed.icon,
                            },
                        ),
                    );
                }
            }
        }
        by_stem.into_values().map(|(_, entry)| entry).collect()
    }

    /// Resolves a .desktop `Icon=` value to a renderable file: absolute
    /// paths pass through; theme names are searched in the standard icon
    /// directories, largest size first.
    fn resolve_desktop_icon(icon: Option<&str>) -> Option<PathBuf> {
        let icon = icon?;
        let icon = icon.trim();
        if icon.is_empty() {
            return None;
        }
        let direct = PathBuf::from(icon);
        if direct.is_absolute() {
            return direct.is_file().then_some(direct);
        }
        // Search the largest raster sizes first, then the scalable and
        // pixmap fallbacks.
        let sizes = [
            "512x512", "256x256", "192x192", "128x128", "96x96", "64x64", "48x48", "32x32",
            "24x24", "16x16",
        ];
        let themes = [
            "/usr/share/icons/hicolor",
            "/usr/local/share/icons/hicolor",
            "/var/lib/flatpak/exports/share/icons/hicolor",
            "/var/lib/snapd/desktop/icons/hicolor",
        ];
        for theme in themes {
            for size in sizes {
                let candidate = PathBuf::from(format!("{theme}/{size}/apps/{icon}.png"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        for extension in ["png", "svg", "xpm"] {
            let candidate = PathBuf::from(format!("/usr/share/pixmaps/{icon}.{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    fn pacman_sizes() -> HashMap<String, u64> {
        let mut sizes = HashMap::new();
        let Some(output) = platform::run_quiet("pacman", &["-Qi"]) else {
            return sizes;
        };
        let mut current: Option<String> = None;
        for line in output.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key == "Name" {
                current = Some(value.to_owned());
            } else if key == "Installed Size" {
                if let (Some(name), Some(bytes)) = (&current, parse_size(value)) {
                    sizes.insert(name.clone(), bytes);
                }
            }
        }
        sizes
    }

    fn dpkg_sizes() -> HashMap<String, u64> {
        let mut sizes = HashMap::new();
        let Some(output) = platform::run_quiet(
            "dpkg-query",
            &["-W", "-f=${Package}\\t${Installed-Size}\\n"],
        ) else {
            return sizes;
        };
        for line in output.lines() {
            let Some((package, kib)) = line.split_once('\t') else {
                continue;
            };
            if let Ok(kib) = kib.trim().parse::<u64>() {
                sizes.insert(package.trim().to_owned(), kib * 1024);
            }
        }
        sizes
    }

    fn rpm_sizes() -> HashMap<String, u64> {
        let mut sizes = HashMap::new();
        let Some(output) = platform::run_quiet("rpm", &["-qa", "--qf", "%{NAME}\\t%{SIZE}\\n"])
        else {
            return sizes;
        };
        for line in output.lines() {
            let Some((package, bytes)) = line.split_once('\t') else {
                continue;
            };
            if let Ok(bytes) = bytes.trim().parse::<u64>() {
                sizes.insert(package.trim().to_owned(), bytes);
            }
        }
        sizes
    }

    /// Snap disk usage is the sum of its revision squashfs files.
    fn snap_sizes() -> HashMap<String, u64> {
        let mut sizes = HashMap::new();
        let Ok(entries) = std::fs::read_dir("/var/lib/snapd/snaps") else {
            return sizes;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(name) = name.strip_suffix(".snap") else {
                continue;
            };
            let Some((snap_name, _revision)) = name.rsplit_once('_') else {
                continue;
            };
            if let Ok(metadata) = entry.metadata() {
                *sizes.entry(snap_name.to_owned()).or_insert(0) += metadata.len();
            }
        }
        sizes
    }

    #[allow(clippy::too_many_arguments)]
    fn build_entry(
        name: String,
        exec: String,
        icon: Option<PathBuf>,
        pacman: &HashMap<String, u64>,
        dpkg: &HashMap<String, u64>,
        rpm: &HashMap<String, u64>,
        snap: &HashMap<String, u64>,
    ) -> Option<AppEntry> {
        match classify_exec(&exec)? {
            ExecTarget::Flatpak(id) => {
                let dir = flatpak_app_dir(&id);
                let size = dir.as_deref().and_then(directory_size);
                let last_used = dir.as_deref().and_then(flatpak_last_used);
                Some(AppEntry {
                    name,
                    icon,
                    path: dir.unwrap_or_default(),
                    size,
                    last_used,
                    uninstall: Some(Uninstall::Flatpak(id)),
                })
            }
            ExecTarget::Snap(snap_name) => {
                let launcher = PathBuf::from("/snap/bin").join(&snap_name);
                Some(AppEntry {
                    name,
                    icon,
                    path: launcher.clone(),
                    size: snap.get(&snap_name).copied(),
                    last_used: last_used_by_atime(&launcher),
                    uninstall: Some(Uninstall::Snap(snap_name)),
                })
            }
            ExecTarget::Binary(path) => {
                let last_used = last_used_by_atime(&path);
                let path_string = path.to_string_lossy().into_owned();
                if let Some(package) = owning_pacman_package(&path_string) {
                    return Some(AppEntry {
                        name,
                        icon,
                        path,
                        size: pacman.get(&package).copied(),
                        last_used,
                        uninstall: Some(Uninstall::Pacman(package)),
                    });
                }
                if let Some(package) = owning_dpkg_package(&path_string) {
                    return Some(AppEntry {
                        name,
                        icon,
                        path,
                        size: dpkg.get(&package).copied(),
                        last_used,
                        uninstall: Some(Uninstall::Dpkg(package)),
                    });
                }
                if let Some(package) = owning_rpm_package(&path_string) {
                    return Some(AppEntry {
                        name,
                        icon,
                        path,
                        size: rpm.get(&package).copied(),
                        last_used,
                        uninstall: Some(Uninstall::Rpm(package)),
                    });
                }
                // Manually installed app outside any package manager.
                Some(AppEntry {
                    name,
                    icon,
                    path,
                    size: None,
                    last_used,
                    uninstall: None,
                })
            }
        }
    }

    fn flatpak_app_dir(id: &str) -> Option<PathBuf> {
        let mut roots = Vec::new();
        if let Some(home) = platform::home_dir() {
            roots.push(home.join(".local/share/flatpak/app"));
        }
        roots.push(PathBuf::from("/var/lib/flatpak/app"));
        roots
            .into_iter()
            .map(|root| root.join(id))
            .find(|dir| dir.is_dir())
    }

    fn directory_size(dir: &std::path::Path) -> Option<u64> {
        let root = dir.to_path_buf();
        let files = crate::scanner::walk_with_dir_exclusions(
            std::slice::from_ref(&root),
            12,
            |_, _| true,
            false,
        );
        Some(files.into_iter().map(|(_, metadata)| metadata.len()).sum())
    }

    /// Flatpaks do not share the host binaries, so the newest atime among the
    /// app's own launchers is the usage signal.
    fn flatpak_last_used(dir: &std::path::Path) -> Option<SystemTime> {
        let bin = dir.join("files/bin");
        let Ok(entries) = std::fs::read_dir(&bin) else {
            return None;
        };
        let mut latest = None;
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if let (Ok(accessed), Ok(modified)) = (metadata.accessed(), metadata.modified()) {
                if accessed > modified && latest.map_or(true, |latest| accessed > latest) {
                    latest = Some(accessed);
                }
            }
        }
        latest
    }

    /// atime under relatime gives the last execution within ~24h. An atime
    /// not newer than mtime means "never used since install".
    fn last_used_by_atime(path: &std::path::Path) -> Option<SystemTime> {
        let metadata = std::fs::metadata(path).ok()?;
        let (Ok(accessed), Ok(modified)) = (metadata.accessed(), metadata.modified()) else {
            return None;
        };
        (accessed > modified).then_some(accessed)
    }

    fn owning_pacman_package(path: &str) -> Option<String> {
        let output = platform::run_quiet("pacman", &["-Qoq", path])?;
        output
            .lines()
            .map(str::trim)
            .next()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
    }

    fn owning_dpkg_package(path: &str) -> Option<String> {
        let output = platform::run_quiet("dpkg-query", &["-S", path])?;
        let line = output.lines().next()?;
        let (packages, _) = line.split_once(':')?;
        let package = packages.split(',').next()?.trim();
        (!package.is_empty()).then(|| package.to_owned())
    }

    fn owning_rpm_package(path: &str) -> Option<String> {
        let output = platform::run_quiet("rpm", &["-qf", "--qf", "%{NAME}", path])?;
        let package = output.lines().next()?.trim();
        (!package.is_empty()).then(|| package.to_owned())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{parse_mdls_date, AppEntry, Uninstall};
    use crate::parallel::parallel_map;
    use std::path::PathBuf;
    use std::time::SystemTime;

    pub fn scan() -> Vec<AppEntry> {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = crate::platform::home_dir() {
            roots.push(home.join("Applications"));
        }
        let bundles: Vec<PathBuf> = roots
            .iter()
            .filter_map(|root| std::fs::read_dir(root).ok())
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir() && path.extension().and_then(|ext| ext.to_str()) == Some("app")
            })
            .collect();
        parallel_map(bundles, |bundle| {
            let name = bundle
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some(AppEntry {
                name,
                icon: None,
                path: bundle.clone(),
                size: directory_size(&bundle),
                last_used: mdls_last_used(&bundle),
                uninstall: Some(Uninstall::MacBundle(bundle)),
            })
        })
        .into_iter()
        .flatten()
        .collect()
    }

    fn directory_size(dir: &std::path::Path) -> Option<u64> {
        let root = dir.to_path_buf();
        let files = crate::scanner::walk_with_dir_exclusions(
            std::slice::from_ref(&root),
            14,
            |_, _| true,
            false,
        );
        Some(files.into_iter().map(|(_, metadata)| metadata.len()).sum())
    }

    fn mdls_last_used(bundle: &std::path::Path) -> Option<SystemTime> {
        let output = std::process::Command::new("mdls")
            .args(["-name", "kMDItemLastUsedDate", "-raw"])
            .arg(bundle)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        parse_mdls_date(text.trim())
    }
}

#[cfg(windows)]
mod windows {
    use super::{last_run_from_userassist, rot13, AppEntry, Uninstall};
    use crate::registry;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::SystemTime;
    use winreg::types::FromRegValue;
    use winreg::RegValue;

    pub fn scan() -> Vec<AppEntry> {
        let mut apps = uninstall_entries();
        let usage = userassist_usage();
        for app in &mut apps {
            app.last_used = app.last_used.or_else(|| match_usage(&usage, app));
        }
        apps
    }

    fn uninstall_entries() -> Vec<AppEntry> {
        let mut apps = Vec::new();
        for root_path in registry::uninstall_roots() {
            let Some(root) = registry::open_key(root_path) else {
                continue;
            };
            for key_name in root.enum_keys().flatten() {
                let key_path = format!(r"{root_path}\{key_name}");
                let Some(key) = registry::open_key(&key_path) else {
                    continue;
                };
                let values: Vec<(String, RegValue)> = key.enum_values().flatten().collect();
                let Some(display_name) =
                    string_value(&values, "DisplayName").filter(|name| !name.is_empty())
                else {
                    continue;
                };
                let Some(uninstall_string) = string_value(&values, "UninstallString")
                    .or_else(|| string_value(&values, "QuietUninstallString"))
                else {
                    continue;
                };
                let install_location = string_value(&values, "InstallLocation");
                let display_icon = string_value(&values, "DisplayIcon");
                let size_kb = dword_value(&values, "EstimatedSize");
                let raw_icon = display_icon
                    .as_deref()
                    .and_then(|icon| icon.split_once(',').map(|(path, _)| path))
                    .map(PathBuf::from);
                // WebView images cannot come from inside an .exe — only
                // standalone .ico/.png icons are usable.
                let is_image_icon = raw_icon.as_ref().is_some_and(|icon| {
                    icon.extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| {
                            matches!(ext.to_ascii_lowercase().as_str(), "ico" | "png")
                        })
                });
                let icon = if is_image_icon {
                    raw_icon.clone()
                } else {
                    None
                };
                let path = raw_icon
                    .filter(|icon| !icon.as_os_str().is_empty())
                    .or_else(|| {
                        install_location
                            .clone()
                            .filter(|l| !l.is_empty())
                            .map(PathBuf::from)
                    });
                apps.push(AppEntry {
                    name: display_name,
                    icon,
                    path: path.unwrap_or_else(|| PathBuf::from(&key_path)),
                    size: size_kb.map(|kb| kb as u64 * 1024),
                    last_used: None,
                    uninstall: Some(Uninstall::Windows(uninstall_string)),
                });
            }
        }
        apps
    }

    /// UserAssist tracks every program launched via Explorer: ROT13-encoded
    /// value names holding the full path, with the last-run FILETIME inside
    /// the binary data.
    fn userassist_usage() -> HashMap<String, SystemTime> {
        const USERASSIST: &str =
            "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\UserAssist";
        let mut usage = HashMap::new();
        let Some(root) = registry::open_key(USERASSIST) else {
            return usage;
        };
        for guid in root.enum_keys().flatten() {
            let count_path = format!(r"{USERASSIST}\{guid}\Count");
            let Some(count) = registry::open_key(&count_path) else {
                continue;
            };
            for (encoded, value) in count.enum_values().flatten() {
                let decoded = rot13(&encoded).to_lowercase();
                if !decoded.ends_with(".exe") && !decoded.ends_with(".lnk") {
                    continue;
                }
                if let Some(last_run) = last_run_from_userassist(&value.bytes) {
                    usage.insert(decoded, last_run);
                }
            }
        }
        usage
    }

    fn match_usage(usage: &HashMap<String, SystemTime>, app: &AppEntry) -> Option<SystemTime> {
        let path = app.path.to_string_lossy().to_lowercase();
        if let Some(time) = usage.get(&path) {
            return Some(*time);
        }
        // Install locations often differ from the launched path; fall back to
        // matching the executable file name.
        let file_name = app
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase());
        let mut matches: Vec<SystemTime> = usage
            .iter()
            .filter(|(key, _)| {
                file_name
                    .as_deref()
                    .is_some_and(|file_name| key.ends_with(file_name))
            })
            .map(|(_, time)| *time)
            .collect();
        // UserAssist records GUI launches by their Start Menu shortcut
        // (e.g. "google chrome.lnk"), not by the executable — the most
        // common reason an app resolved to "never used". Match shortcut
        // stems against the display name in both directions.
        let normalize = |text: &str| {
            text.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        };
        let app_name = normalize(&app.name);
        if app_name.len() >= 3 {
            matches.extend(
                usage
                    .iter()
                    .filter(|(key, _)| key.ends_with(".lnk"))
                    .filter(|(key, _)| {
                        let stem = key
                            .rsplit('\\')
                            .next()
                            .unwrap_or(key)
                            .trim_end_matches(".lnk");
                        let stem = normalize(stem);
                        !stem.is_empty() && (stem.contains(&app_name) || app_name.contains(&stem))
                    })
                    .map(|(_, time)| *time),
            );
        }
        matches.into_iter().max()
    }

    fn string_value(values: &[(String, RegValue)], name: &str) -> Option<String> {
        let (_, value) = values.iter().find(|(key, _)| key == name)?;
        String::from_reg_value(value).ok()
    }

    fn dword_value(values: &[(String, RegValue)], name: &str) -> Option<u32> {
        let (_, value) = values.iter().find(|(key, _)| key == name)?;
        u32::from_reg_value(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_exec, days_from_civil, filetime_to_system_time, find_in_path,
        last_run_from_userassist, parse_desktop_entry, parse_mdls_date, parse_size, rot13,
        ExecTarget, Uninstall,
    };
    use std::time::{Duration, SystemTime};

    #[test]
    fn parses_valid_desktop_entries_and_rejects_hidden_ones() {
        let valid =
            "[Desktop Entry]\nType=Application\nName=Text Editor\nExec=/usr/bin/editor %f\n";
        let parsed = parse_desktop_entry(valid).unwrap();
        assert_eq!(parsed.name, "Text Editor");
        assert_eq!(parsed.exec, "/usr/bin/editor %f");

        assert!(parse_desktop_entry("[Desktop Entry]\nType=Link\nName=X\nExec=x\n").is_none());
        assert!(parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nNoDisplay=true\nName=X\nExec=x\n"
        )
        .is_none());
        assert!(parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nHidden=true\nName=X\nExec=x\n"
        )
        .is_none());
        assert!(
            parse_desktop_entry("[Other Section]\nType=Application\nName=X\nExec=x\n").is_none()
        );
    }

    #[test]
    fn prefers_the_default_name_over_localized_variants() {
        let text = "[Desktop Entry]\nType=Application\nName[de]=Editor\nName=Editor\nExec=x\n";
        let parsed = parse_desktop_entry(text).unwrap();
        assert_eq!(parsed.name, "Editor");
    }

    #[test]
    fn classifies_flatpak_snap_and_native_execs() {
        let flatpak = "/usr/bin/flatpak run --branch=stable --arch=x86_64 --file-forwarding org.mozilla.firefox @@u %u @@";
        match classify_exec(flatpak) {
            Some(ExecTarget::Flatpak(id)) => assert_eq!(id, "org.mozilla.firefox"),
            other => panic!("expected flatpak, got {other:?}"),
        }
        match classify_exec("/usr/bin/snap run firefox") {
            Some(ExecTarget::Snap(name)) => assert_eq!(name, "firefox"),
            other => panic!("expected snap, got {other:?}"),
        }
        let editor = std::env::temp_dir().join("rusty-cleaner-exec-editor");
        std::fs::write(&editor, b"bin").unwrap();
        match classify_exec(&format!("env FOO=bar {}", editor.display())) {
            Some(ExecTarget::Binary(path)) => assert_eq!(path, editor),
            other => panic!("expected binary, got {other:?}"),
        }
        std::fs::remove_file(&editor).unwrap();
    }

    #[test]
    fn finds_programs_on_the_path() {
        if cfg!(windows) {
            assert!(find_in_path("cmd").is_some());
        } else {
            assert!(find_in_path("sh").is_some());
        }
        assert!(find_in_path("rusty-cleaner-no-such-program-xyz").is_none());
    }

    #[test]
    fn parses_pacman_style_sizes() {
        assert_eq!(parse_size("12.00 MiB"), Some(12 * 1024 * 1024));
        assert_eq!(parse_size("1.50 GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("245 B"), Some(245));
        assert_eq!(parse_size("garbage"), None);
        assert_eq!(parse_size(""), None);
    }

    #[test]
    fn rot13_round_trips_userassist_names() {
        assert_eq!(rot13("URYYB"), "HELLO");
        assert_eq!(rot13("HELLO"), "URYYB");
        assert_eq!(rot13(&rot13(r"C:\Path\App.exe")), r"C:\Path\App.exe");
    }

    #[test]
    fn converts_windows_filetime_to_system_time() {
        // 1970-01-01 as FILETIME.
        let epoch = filetime_to_system_time(116_444_736_000_000_000).unwrap();
        assert_eq!(epoch, SystemTime::UNIX_EPOCH);
        // Pre-1970 ticks underflow into None instead of panicking.
        assert!(filetime_to_system_time(0).is_none());
    }

    #[test]
    fn extracts_last_run_from_userassist_data() {
        let ticks: u64 = (SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 11_644_473_600)
            * 10_000_000;
        let mut bytes = vec![0_u8; 76];
        bytes[64..72].copy_from_slice(&ticks.to_le_bytes());
        assert!(last_run_from_userassist(&bytes).is_some());

        // Random bytes never surface as a fake date.
        assert!(last_run_from_userassist(&[7_u8; 76]).is_none());
        assert!(last_run_from_userassist(&[0_u8; 16]).is_none());
    }

    #[test]
    fn converts_civil_dates_to_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn parses_spotlight_last_used_dates() {
        let parsed = parse_mdls_date("2026-09-28 09:12:34 +0000").unwrap();
        let elapsed = parsed.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let expected = (days_from_civil(2026, 9, 28) * 86_400 + 9 * 3_600 + 12 * 60 + 34) as u64;
        assert_eq!(elapsed, Duration::from_secs(expected));
        assert!(parse_mdls_date("(null)").is_none());
        assert!(parse_mdls_date("garbage").is_none());
    }

    #[test]
    fn round_trips_uninstall_actions() {
        for (kind, action) in [
            ("pacman", Uninstall::Pacman("firefox".to_owned())),
            ("dpkg", Uninstall::Dpkg("firefox".to_owned())),
            ("rpm", Uninstall::Rpm("firefox".to_owned())),
            (
                "flatpak",
                Uninstall::Flatpak("org.mozilla.firefox".to_owned()),
            ),
            ("snap", Uninstall::Snap("firefox".to_owned())),
            (
                "windows",
                Uninstall::Windows(r"MsiExec.exe /X{GUID}".to_owned()),
            ),
            (
                "macos",
                Uninstall::MacBundle("/Applications/Foo.app".into()),
            ),
        ] {
            assert_eq!(Uninstall::parse(kind, action.arg_for_test()), Some(action));
        }
        assert!(Uninstall::parse("unknown-kind", "x").is_none());
        assert!(Uninstall::parse("pacman", "").is_none());
    }

    impl Uninstall {
        fn arg_for_test(&self) -> &str {
            match self {
                Self::Pacman(arg)
                | Self::Dpkg(arg)
                | Self::Rpm(arg)
                | Self::Flatpak(arg)
                | Self::Snap(arg)
                | Self::Windows(arg) => arg,
                Self::MacBundle(path) => path.to_str().unwrap_or_default(),
            }
        }
    }
}
