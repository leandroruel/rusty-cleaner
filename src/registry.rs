use crate::scanner::{Feature, Finding};
use std::path::{Path, PathBuf};

/// Read access to the Windows Registry, abstracted behind a trait so the
/// cleaning-rule logic below can be unit-tested without Windows. Each key is
/// a full path prefixed with its hive, e.g.
/// `HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`.
pub trait RegistryReader {
    fn subkeys(&self, key: &str) -> Vec<String>;
    fn values(&self, key: &str) -> Vec<(String, String)>;
}

/// One registry problem found by a cleaning rule: either a single value to
/// remove (`value: Some(value_name)`) or a whole key to remove
/// (`value: None`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Issue {
    key: String,
    value: Option<String>,
}

type RuleCheck = fn(&dyn RegistryReader) -> Vec<Issue>;

/// The registry cleaning rules, mirroring the CCleaner Registry Cleaner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    MissingSharedDlls,
    UnusedFileExtensions,
    ActiveXAndClassIssues,
    TypeLibraries,
    Applications,
    Fonts,
    ApplicationPaths,
    HelpFiles,
    Installer,
    ObsoleteSoftware,
    RunAtStartup,
    StartMenuOrdering,
    MuiCache,
    SoundEvents,
    WindowsServices,
}

impl Rule {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "missing-shared-dlls" => Some(Self::MissingSharedDlls),
            "unused-file-extensions" => Some(Self::UnusedFileExtensions),
            "activex-class" => Some(Self::ActiveXAndClassIssues),
            "type-libraries" => Some(Self::TypeLibraries),
            "applications" => Some(Self::Applications),
            "fonts" => Some(Self::Fonts),
            "application-paths" => Some(Self::ApplicationPaths),
            "help-files" => Some(Self::HelpFiles),
            "installer" => Some(Self::Installer),
            "obsolete-software" => Some(Self::ObsoleteSoftware),
            "run-at-startup" => Some(Self::RunAtStartup),
            "start-menu-ordering" => Some(Self::StartMenuOrdering),
            "mui-cache" => Some(Self::MuiCache),
            "sound-events" => Some(Self::SoundEvents),
            "windows-services" => Some(Self::WindowsServices),
            _ => None,
        }
    }

    /// Every rule, in scan order.
    pub fn all() -> &'static [Rule] {
        &[
            Self::MissingSharedDlls,
            Self::UnusedFileExtensions,
            Self::ActiveXAndClassIssues,
            Self::TypeLibraries,
            Self::Applications,
            Self::Fonts,
            Self::ApplicationPaths,
            Self::HelpFiles,
            Self::Installer,
            Self::ObsoleteSoftware,
            Self::RunAtStartup,
            Self::StartMenuOrdering,
            Self::MuiCache,
            Self::SoundEvents,
            Self::WindowsServices,
        ]
    }
}

fn check_for(rule: Rule) -> RuleCheck {
    match rule {
        Rule::MissingSharedDlls => check_missing_shared_dlls,
        Rule::UnusedFileExtensions => check_unused_file_extensions,
        Rule::ActiveXAndClassIssues => check_activex_and_class_issues,
        Rule::TypeLibraries => check_type_libraries,
        Rule::Applications => check_applications,
        Rule::Fonts => check_fonts,
        Rule::ApplicationPaths => check_application_paths,
        Rule::HelpFiles => check_help_files,
        Rule::Installer => check_installer,
        Rule::ObsoleteSoftware => check_obsolete_software,
        Rule::RunAtStartup => check_run_at_startup,
        Rule::StartMenuOrdering => check_start_menu_ordering,
        Rule::MuiCache => check_mui_cache,
        Rule::SoundEvents => check_sound_events,
        Rule::WindowsServices => check_windows_services,
    }
}

/// Reader that always reports an empty registry. Used on platforms without a
/// Windows Registry, where every rule then yields no issues.
#[cfg(not(windows))]
struct EmptyReader;

#[cfg(not(windows))]
impl RegistryReader for EmptyReader {
    fn subkeys(&self, _key: &str) -> Vec<String> {
        Vec::new()
    }

    fn values(&self, _key: &str) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// Runs the selected cleaning rules against the live Windows Registry.
/// Returns an empty list on other platforms, where no registry exists.
pub fn scan_rules(selected: &[Rule]) -> Vec<Finding> {
    #[cfg(windows)]
    let reader = &windows_impl::WinregReader as &dyn RegistryReader;
    #[cfg(not(windows))]
    let reader = &EmptyReader as &dyn RegistryReader;
    selected
        .iter()
        .flat_map(|rule| check_for(*rule)(reader))
        .map(registry_finding)
        .collect()
}

/// Runs every cleaning rule. Convenience wrapper for the CLI.
pub fn scan() -> Vec<Finding> {
    scan_rules(Rule::all())
}

/// Keys removed by a registry fix and the ones that failed, with errors.
#[derive(Debug, Default)]
pub struct FixOutcome {
    pub fixed: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Removes the flagged registry values (`Some(value_name)`) or keys (`None`).
/// Items are `(key, value)` pairs. Only supported on Windows; on other
/// platforms every item is reported as failed.
pub fn fix(items: &[(String, Option<String>)]) -> FixOutcome {
    #[cfg(windows)]
    {
        windows_impl::fix(items)
    }
    #[cfg(not(windows))]
    {
        FixOutcome {
            fixed: Vec::new(),
            failed: items
                .iter()
                .map(|(key, _)| {
                    (
                        key.clone(),
                        "Registry cleaning is only supported on Windows".to_owned(),
                    )
                })
                .collect(),
        }
    }
}

fn registry_finding(issue: Issue) -> Finding {
    Finding {
        feature: Feature::Registry,
        name: issue
            .value
            .clone()
            .unwrap_or_else(|| key_leaf(&issue.key).to_owned()),
        path: PathBuf::from(issue.key),
        size: 0,
        modified: None,
        meta: issue.value,
    }
}

fn key_leaf(key: &str) -> &str {
    key.rsplit('\\').next().unwrap_or(key)
}

fn default_value(reader: &dyn RegistryReader, key: &str) -> Option<String> {
    reader
        .values(key)
        .into_iter()
        .find(|(name, data)| name.is_empty() && !data.is_empty())
        .map(|(_, data)| data)
}

/// Expands `%VAR%` environment references. Unknown variables are kept
/// literally, so plain paths pass through unchanged on every platform.
fn expand_windows_env(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            result.push_str(rest);
            return result;
        };
        let name = &after[..end];
        result.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(value) => result.push_str(&value),
            Err(_) => result.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    result
}

/// Whether a plain path (registry value data, font file, ...) exists. Empty
/// data never counts as an existing path.
fn file_exists(raw: &str) -> bool {
    let path = expand_windows_env(raw.trim().trim_matches('"'));
    !path.is_empty() && Path::new(&path).exists()
}

/// Whether the executable a command line refers to exists. Handles quoted
/// paths (`"C:\Program Files\App\app.exe" -flag`) and unquoted paths with
/// arguments by accepting the longest existing space-delimited prefix, which
/// avoids false positives for paths containing spaces.
fn command_target_exists(command: &str) -> bool {
    let trimmed = command.trim();
    if let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.split_once('"'))
        .map(|(inner, _)| inner)
    {
        return file_exists(inner);
    }
    let expanded = expand_windows_env(trimmed);
    if file_exists(&expanded) {
        return true;
    }
    let mut candidate = String::new();
    for word in expanded.split(' ') {
        if !candidate.is_empty() {
            candidate.push(' ');
        }
        candidate.push_str(word);
        if file_exists(&candidate) {
            return true;
        }
    }
    false
}

/// Removes entries for libraries that are shared by multiple programs but no
/// longer exist. Each value name under `SharedDLLs` is a file path.
fn check_missing_shared_dlls(reader: &dyn RegistryReader) -> Vec<Issue> {
    const KEY: &str =
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\SharedDLLs";
    reader
        .values(KEY)
        .into_iter()
        .filter(|(_, data)| !file_exists(data))
        .map(|(name, _)| Issue {
            key: KEY.to_owned(),
            value: Some(name),
        })
        .collect()
}

/// Removes file extension keys that are no longer associated with any
/// application: no ProgID default value and no subkeys at all.
fn check_unused_file_extensions(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str = "HKEY_CLASSES_ROOT";
    let mut issues = Vec::new();
    for name in reader.subkeys(ROOT) {
        if !name.starts_with('.') {
            continue;
        }
        let key = format!(r"{ROOT}\{name}");
        let has_association = reader
            .values(&key)
            .iter()
            .any(|(value, data)| value.is_empty() && !data.is_empty())
            || !reader.subkeys(&key).is_empty();
        if !has_association {
            issues.push(Issue { key, value: None });
        }
    }
    issues
}

/// Removes COM/ActiveX class entries whose server executables or libraries no
/// longer exist.
fn check_activex_and_class_issues(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str = "HKEY_CLASSES_ROOT\\CLSID";
    let mut issues = Vec::new();
    for guid in reader.subkeys(ROOT) {
        let class_key = format!(r"{ROOT}\{guid}");
        let server_missing = ["InprocServer32", "LocalServer32"].iter().any(|server| {
            reader
                .values(&format!(r"{class_key}\{server}"))
                .iter()
                .any(|(value, data)| value.is_empty() && !data.is_empty() && !file_exists(data))
        });
        if server_missing {
            issues.push(Issue {
                key: class_key,
                value: None,
            });
        }
    }
    issues
}

/// Removes type library registrations whose referenced files no longer exist.
fn check_type_libraries(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str = "HKEY_CLASSES_ROOT\\TypeLib";
    let mut issues = Vec::new();
    for guid in reader.subkeys(ROOT) {
        let lib_key = format!(r"{ROOT}\{guid}");
        for version in reader.subkeys(&lib_key) {
            let version_key = format!(r"{lib_key}\{version}");
            for platform in ["win32", "win64"] {
                let platform_key = format!(r"{version_key}\{platform}");
                let invalid = reader.values(&platform_key).iter().any(|(value, data)| {
                    value.is_empty() && !data.is_empty() && !file_exists(data)
                });
                if invalid {
                    issues.push(Issue {
                        key: platform_key,
                        value: None,
                    });
                }
            }
        }
    }
    issues
}

/// Removes application registration entries whose launch command points to a
/// non-existing executable.
fn check_applications(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str = "HKEY_CLASSES_ROOT\\Applications";
    let mut issues = Vec::new();
    for exe in reader.subkeys(ROOT) {
        let command_key = format!(r"{ROOT}\{exe}\shell\open\command");
        let Some(command) = default_value(reader, &command_key) else {
            continue;
        };
        if !command_target_exists(&command) {
            issues.push(Issue {
                key: format!(r"{ROOT}\{exe}"),
                value: None,
            });
        }
    }
    issues
}

/// Removes font registrations whose files no longer exist in the Windows
/// Fonts folder. Value data is either absolute or relative to that folder.
fn check_fonts(reader: &dyn RegistryReader) -> Vec<Issue> {
    check_fonts_with_dir(reader, Path::new("C:\\Windows\\Fonts"))
}

fn check_fonts_with_dir(reader: &dyn RegistryReader, fonts_dir: &Path) -> Vec<Issue> {
    const KEY: &str = "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts";
    reader
        .values(KEY)
        .into_iter()
        .filter(|(_, data)| {
            let file = if data.contains(':') {
                PathBuf::from(expand_windows_env(data.trim()))
            } else {
                fonts_dir.join(data)
            };
            !file.exists()
        })
        .map(|(name, _)| Issue {
            key: KEY.to_owned(),
            value: Some(name),
        })
        .collect()
}

/// Removes registered application paths that no longer exist. The default
/// value of each `App Paths` subkey is the executable path.
fn check_application_paths(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str =
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths";
    let mut issues = Vec::new();
    for exe in reader.subkeys(ROOT) {
        let key = format!(r"{ROOT}\{exe}");
        let Some(target) = default_value(reader, &key) else {
            continue;
        };
        if !file_exists(&target) {
            issues.push(Issue { key, value: None });
        }
    }
    issues
}

/// Removes entries referring to help files that no longer exist. Each value's
/// data under the `Help` key is a help file path.
fn check_help_files(reader: &dyn RegistryReader) -> Vec<Issue> {
    const KEY: &str = "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Help";
    reader
        .values(KEY)
        .into_iter()
        .filter(|(_, data)| !file_exists(data))
        .map(|(name, _)| Issue {
            key: KEY.to_owned(),
            value: Some(name),
        })
        .collect()
}

fn uninstall_roots() -> &'static [&'static str] {
    &[
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ]
}

/// Removes the remnants of leftover uninstallations: uninstall entries whose
/// uninstall executable no longer exists.
fn check_installer(reader: &dyn RegistryReader) -> Vec<Issue> {
    let mut issues = Vec::new();
    for root in uninstall_roots() {
        for entry in reader.subkeys(root) {
            let key = format!(r"{root}\{entry}");
            let command = reader
                .values(&key)
                .iter()
                .find(|(name, data)| {
                    (name == "UninstallString" || name == "QuietUninstallString")
                        && !data.is_empty()
                })
                .map(|(_, data)| data.clone());
            let Some(command) = command else {
                continue;
            };
            if !command_target_exists(&command) {
                issues.push(Issue { key, value: None });
            }
        }
    }
    issues
}

/// Removes entries for applications that are no longer installed. Only leaf
/// keys whose values reference files that no longer exist are flagged —
/// umbrella keys (e.g. "Microsoft") hold live settings for many programs and
/// must never be removed.
fn check_obsolete_software(reader: &dyn RegistryReader) -> Vec<Issue> {
    let mut issues = Vec::new();
    for root in [
        "HKEY_CURRENT_USER\\SOFTWARE",
        "HKEY_LOCAL_MACHINE\\SOFTWARE",
    ] {
        for name in reader.subkeys(root) {
            let key = format!(r"{root}\{name}");
            if !reader.subkeys(&key).is_empty() {
                continue;
            }
            let references_missing_file = reader
                .values(&key)
                .iter()
                .any(|(_, data)| data.contains('\\') && !file_exists(data));
            if references_missing_file {
                issues.push(Issue { key, value: None });
            }
        }
    }
    issues
}

fn startup_keys() -> &'static [&'static str] {
    &[
        "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
        "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run",
    ]
}

/// Removes startup entries whose executables no longer exist. Each value is
/// one startup command.
fn check_run_at_startup(reader: &dyn RegistryReader) -> Vec<Issue> {
    let mut issues = Vec::new();
    for root in startup_keys() {
        for (name, data) in reader.values(root) {
            if !command_target_exists(&data) {
                issues.push(Issue {
                    key: root.to_string(),
                    value: Some(name),
                });
            }
        }
    }
    issues
}

const MENU_ORDER: &str =
    "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\MenuOrder";

/// Removes Start Menu ordering keys whose folders no longer exist. `Start
/// Menu` refers to the per-user folder, `Start Menu2` to the common one.
fn check_start_menu_ordering(reader: &dyn RegistryReader) -> Vec<Issue> {
    let user = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|base| base.join("Microsoft\\Windows\\Start Menu"));
    let common = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|base| base.join("Microsoft\\Windows\\Start Menu"));
    match (user, common) {
        (Some(user), Some(common)) => start_menu_ordering_issues(reader, &user, &common),
        _ => Vec::new(),
    }
}

/// Recursively checks `MenuOrder` keys against the Start Menu folders they
/// refer to. The two Start Menu roots are injected so the mapping is testable
/// without Windows.
fn start_menu_ordering_issues(
    reader: &dyn RegistryReader,
    user_base: &Path,
    common_base: &Path,
) -> Vec<Issue> {
    fn visit(
        reader: &dyn RegistryReader,
        user_base: &Path,
        common_base: &Path,
        relative: &str,
        issues: &mut Vec<Issue>,
    ) {
        let key = format!(r"{MENU_ORDER}\{relative}");
        let (first, rest) = match relative.split_once('\\') {
            Some((first, rest)) => (first, Some(rest)),
            None => (relative, None),
        };
        let folder = match (first, rest) {
            ("Start Menu", Some(rest)) => Some(user_base.join(rest.replace('\\', "/"))),
            ("Start Menu", None) => Some(user_base.to_path_buf()),
            ("Start Menu2", Some(rest)) => Some(common_base.join(rest.replace('\\', "/"))),
            ("Start Menu2", None) => Some(common_base.to_path_buf()),
            _ => None,
        };
        match folder {
            Some(folder) if folder.exists() => {
                for sub in reader.subkeys(&key) {
                    visit(
                        reader,
                        user_base,
                        common_base,
                        &format!(r"{relative}\{sub}"),
                        issues,
                    );
                }
            }
            _ => issues.push(Issue { key, value: None }),
        }
    }
    let mut issues = Vec::new();
    for top in reader.subkeys(MENU_ORDER) {
        visit(reader, user_base, common_base, &top, &mut issues);
    }
    issues
}

/// Removes invalid entries from the cache of recently run programs. Value
/// names look like `C:\Path\App.exe.FriendlyAppName`.
fn check_mui_cache(reader: &dyn RegistryReader) -> Vec<Issue> {
    const KEY: &str = "HKEY_CURRENT_USER\\SOFTWARE\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\Shell\\MuiCache";
    reader
        .values(KEY)
        .into_iter()
        .filter(|(name, _)| {
            name.rsplit_once('.')
                .is_some_and(|(target, _suffix)| !file_exists(target))
        })
        .map(|(name, _)| Issue {
            key: KEY.to_owned(),
            value: Some(name),
        })
        .collect()
}

/// Removes references to sound files that do not exist, under each sound
/// event's scheme (`.current`, `.Default`, ...).
fn check_sound_events(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str = "HKEY_CURRENT_USER\\AppEvents\\Schemes\\Apps";
    let mut issues = Vec::new();
    for app in reader.subkeys(ROOT) {
        let app_key = format!(r"{ROOT}\{app}");
        for event in reader.subkeys(&app_key) {
            let event_key = format!(r"{app_key}\{event}");
            for scheme in reader.subkeys(&event_key) {
                let scheme_key = format!(r"{event_key}\{scheme}");
                for (name, data) in reader.values(&scheme_key) {
                    if !file_exists(&data) {
                        issues.push(Issue {
                            key: scheme_key.clone(),
                            value: Some(name),
                        });
                    }
                }
            }
        }
    }
    issues
}

/// Removes Windows Services whose executables are no longer present. The
/// `ImagePath` value of each service is its executable command line.
fn check_windows_services(reader: &dyn RegistryReader) -> Vec<Issue> {
    const ROOT: &str = "HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Services";
    let mut issues = Vec::new();
    for service in reader.subkeys(ROOT) {
        let key = format!(r"{ROOT}\{service}");
        let values = reader.values(&key);
        let Some((_, image_path)) = values.iter().find(|(name, _)| name == "ImagePath") else {
            continue;
        };
        if !command_target_exists(image_path) {
            issues.push(Issue { key, value: None });
        }
    }
    issues
}

#[cfg(windows)]
mod windows_impl {
    use super::{FixOutcome, RegistryReader};
    use winreg::enums::{
        HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE,
    };
    use winreg::types::FromRegValue;
    use winreg::{RegKey, HKEY};

    pub struct WinregReader;

    fn split_root(key: &str) -> Option<(HKEY, &str)> {
        let (hive, rest) = if let Some(rest) = key.strip_prefix("HKEY_LOCAL_MACHINE") {
            (HKEY_LOCAL_MACHINE, rest)
        } else if let Some(rest) = key.strip_prefix("HKEY_CURRENT_USER") {
            (HKEY_CURRENT_USER, rest)
        } else if let Some(rest) = key.strip_prefix("HKEY_CLASSES_ROOT") {
            (HKEY_CLASSES_ROOT, rest)
        } else {
            return None;
        };
        Some((hive, rest.trim_start_matches('\\')))
    }

    fn open(key: &str, perms: u32) -> Option<RegKey> {
        let (hive, rest) = split_root(key)?;
        let hive_key = RegKey::predef(hive);
        if rest.is_empty() {
            Some(hive_key)
        } else {
            hive_key.open_subkey_with_flags(rest, perms).ok()
        }
    }

    impl RegistryReader for WinregReader {
        fn subkeys(&self, key: &str) -> Vec<String> {
            let Some(sub) = open(key, KEY_READ) else {
                return Vec::new();
            };
            sub.enum_keys().filter_map(Result::ok).collect()
        }

        fn values(&self, key: &str) -> Vec<(String, String)> {
            let Some(sub) = open(key, KEY_READ) else {
                return Vec::new();
            };
            sub.enum_values()
                .filter_map(Result::ok)
                .filter_map(|(name, value)| {
                    String::from_reg_value(&value).ok().map(|data| (name, data))
                })
                .collect()
        }
    }

    fn delete_value(key: &str, value_name: &str) -> Result<(), String> {
        let (hive, rest) =
            split_root(key).ok_or_else(|| format!("Unsupported registry hive: {key}"))?;
        if rest.is_empty() {
            return Err("Cannot delete values directly under a hive root".to_owned());
        }
        let hive_key = RegKey::predef(hive);
        let sub = hive_key
            .open_subkey_with_flags(rest, KEY_WRITE)
            .map_err(|error| error.to_string())?;
        sub.delete_value(value_name)
            .map_err(|error| error.to_string())
    }

    fn delete_key_tree(key: &str) -> Result<(), String> {
        let (hive, rest) =
            split_root(key).ok_or_else(|| format!("Unsupported registry hive: {key}"))?;
        if rest.is_empty() {
            return Err("Cannot delete a registry hive root".to_owned());
        }
        let (parent, last) = rest
            .rsplit_once('\\')
            .ok_or_else(|| "Cannot delete a top-level registry key".to_owned())?;
        let hive_key = RegKey::predef(hive);
        let parent_key = hive_key
            .open_subkey_with_flags(parent, KEY_WRITE)
            .map_err(|error| error.to_string())?;
        parent_key
            .delete_subkey_all(last)
            .map_err(|error| error.to_string())
    }

    pub fn fix(items: &[(String, Option<String>)]) -> FixOutcome {
        let mut outcome = FixOutcome::default();
        for (key, value) in items {
            let result = match value {
                Some(value_name) => delete_value(key, value_name),
                None => delete_key_tree(key),
            };
            match result {
                Ok(()) => outcome.fixed.push(key.clone()),
                Err(error) => outcome.failed.push((key.clone(), error)),
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_activex_and_class_issues, check_application_paths, check_fonts_with_dir,
        check_installer, check_missing_shared_dlls, check_mui_cache, check_run_at_startup,
        check_sound_events, check_unused_file_extensions, check_windows_services, fix,
        start_menu_ordering_issues, RegistryReader,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Mock {
        subkeys: HashMap<String, Vec<String>>,
        values: HashMap<String, Vec<(String, String)>>,
    }

    impl Mock {
        fn new() -> Self {
            Self {
                subkeys: HashMap::new(),
                values: HashMap::new(),
            }
        }

        fn subkey(mut self, key: &str, names: &[&str]) -> Self {
            self.subkeys.insert(
                key.to_owned(),
                names.iter().map(|name| name.to_string()).collect(),
            );
            self
        }

        fn value(mut self, key: &str, name: &str, data: &str) -> Self {
            self.values
                .entry(key.to_owned())
                .or_default()
                .push((name.to_owned(), data.to_owned()));
            self
        }
    }

    impl RegistryReader for Mock {
        fn subkeys(&self, key: &str) -> Vec<String> {
            self.subkeys.get(key).cloned().unwrap_or_default()
        }

        fn values(&self, key: &str) -> Vec<(String, String)> {
            self.values.get(key).cloned().unwrap_or_default()
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rusty-cleaner-reg-{}-{nonce}-{name}",
            std::process::id()
        ));
        fs::write(&path, b"data").unwrap();
        path
    }

    #[test]
    fn flags_only_shared_dlls_that_no_longer_exist() {
        let existing = temp_file("shared.dll");
        let shared_dlls =
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\SharedDLLs";
        let mock = Mock::new()
            .value(
                shared_dlls,
                "C:\\missing\\gone.dll",
                "C:\\missing\\gone.dll",
            )
            .value(
                shared_dlls,
                &existing.to_string_lossy(),
                &existing.to_string_lossy(),
            );

        let issues = check_missing_shared_dlls(&mock);
        fs::remove_file(&existing).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].value.as_deref(), Some("C:\\missing\\gone.dll"));
    }

    #[test]
    fn flags_startup_entries_pointing_to_missing_executables() {
        let existing = temp_file("app.exe");
        let run = "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
        let mock = Mock::new()
            .value(run, "MissingApp", "C:\\missing\\app.exe")
            .value(run, "QuotedApp", "\"C:\\missing dir\\app.exe\" -mini")
            .value(
                run,
                "ExistingApp",
                &format!("\"{}\" --flag", existing.display()),
            )
            .value(
                run,
                "UnquotedExisting",
                &format!("{} -k group", existing.display()),
            );

        let issues = check_run_at_startup(&mock);
        fs::remove_file(&existing).unwrap();

        let flagged: Vec<String> = issues
            .iter()
            .filter_map(|issue| issue.value.clone())
            .collect();
        assert_eq!(flagged, vec!["MissingApp", "QuotedApp"]);
    }

    #[test]
    fn flags_unassociated_file_extensions() {
        let mock = Mock::new()
            .subkey("HKEY_CLASSES_ROOT", &[".abc", ".def"])
            .value("HKEY_CLASSES_ROOT\\.def", "", "deffile");

        let issues = check_unused_file_extensions(&mock);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "HKEY_CLASSES_ROOT\\.abc");
        assert!(issues[0].value.is_none());
    }

    #[test]
    fn flags_fonts_missing_from_the_fonts_folder() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fonts_dir = std::env::temp_dir().join(format!(
            "rusty-cleaner-fonts-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&fonts_dir).unwrap();
        fs::write(fonts_dir.join("real.ttf"), b"font").unwrap();
        let fonts_key =
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts";
        let mock = Mock::new()
            .value(fonts_key, "Missing Font (TrueType)", "missing.ttf")
            .value(fonts_key, "Real Font (TrueType)", "real.ttf");

        let issues = check_fonts_with_dir(&mock, &fonts_dir);
        fs::remove_dir_all(&fonts_dir).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].value.as_deref(), Some("Missing Font (TrueType)"));
    }

    #[test]
    fn flags_application_paths_that_no_longer_exist() {
        let app_paths =
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths";
        let mock = Mock::new().subkey(app_paths, &["missing.exe"]).value(
            &format!(r"{app_paths}\missing.exe"),
            "",
            "C:\\missing\\missing.exe",
        );

        let issues = check_application_paths(&mock);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, format!(r"{app_paths}\missing.exe"));
        assert!(issues[0].value.is_none());
    }

    #[test]
    fn flags_uninstaller_entries_with_missing_uninstallers() {
        let existing = temp_file("unins.exe");
        let uninstall =
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
        let mock = Mock::new()
            .subkey(uninstall, &["BrokenApp", "GoodApp"])
            .value(
                &format!(r"{uninstall}\BrokenApp"),
                "UninstallString",
                "C:\\missing\\unins.exe",
            )
            .value(
                &format!(r"{uninstall}\GoodApp"),
                "UninstallString",
                &existing.to_string_lossy(),
            );

        let issues = check_installer(&mock);
        fs::remove_file(&existing).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, format!(r"{uninstall}\BrokenApp"));
        assert!(issues[0].value.is_none());
    }

    #[test]
    fn flags_class_entries_pointing_to_missing_servers() {
        let clsid = "HKEY_CLASSES_ROOT\\CLSID";
        let guid = "{11111111-1111-1111-1111-111111111111}";
        let mock = Mock::new().subkey(clsid, &[guid]).value(
            &format!(r"{clsid}\{guid}\InprocServer32"),
            "",
            "C:\\missing\\server.dll",
        );

        let issues = check_activex_and_class_issues(&mock);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, format!(r"{clsid}\{guid}"));
        assert!(issues[0].value.is_none());
    }

    #[test]
    fn flags_mui_cache_entries_for_missing_programs() {
        let existing = temp_file("app.exe");
        let mui_cache = "HKEY_CURRENT_USER\\SOFTWARE\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\Shell\\MuiCache";
        let mock = Mock::new()
            .value(mui_cache, "C:\\missing\\gone.exe.FriendlyAppName", "Gone")
            .value(
                mui_cache,
                &format!("{}.FriendlyAppName", existing.display()),
                "Here",
            );

        let issues = check_mui_cache(&mock);
        fs::remove_file(&existing).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].value.as_deref(),
            Some("C:\\missing\\gone.exe.FriendlyAppName")
        );
    }

    #[test]
    fn flags_services_with_missing_executables() {
        let existing = temp_file("svc.exe");
        let services = "HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Services";
        let mock = Mock::new()
            .subkey(services, &["BrokenSvc", "WorkingSvc"])
            .value(
                &format!(r"{services}\BrokenSvc"),
                "ImagePath",
                "C:\\missing\\svc.exe",
            )
            .value(
                &format!(r"{services}\WorkingSvc"),
                "ImagePath",
                &format!("{} -k group", existing.display()),
            );

        let issues = check_windows_services(&mock);
        fs::remove_file(&existing).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, format!(r"{services}\BrokenSvc"));
        assert!(issues[0].value.is_none());
    }

    #[test]
    fn flags_sound_events_pointing_to_missing_files() {
        let existing = temp_file("sound.wav");
        let apps = "HKEY_CURRENT_USER\\AppEvents\\Schemes\\Apps";
        let scheme = format!(r"{apps}\Explorer\Navigating\.current");
        let mock = Mock::new()
            .subkey(apps, &["Explorer"])
            .subkey(&format!(r"{apps}\Explorer"), &["Navigating"])
            .subkey(&format!(r"{apps}\Explorer\Navigating"), &[".current"])
            .value(&scheme, "", "C:\\missing\\gone.wav")
            .value(&scheme, "existing", &existing.to_string_lossy());

        let issues = check_sound_events(&mock);
        fs::remove_file(&existing).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, scheme);
        assert_eq!(issues[0].value.as_deref(), Some(""));
    }

    #[test]
    fn flags_start_menu_ordering_keys_with_missing_folders() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rusty-cleaner-menu-{}-{nonce}", std::process::id()));
        let user_base = root.join("Start Menu");
        fs::create_dir_all(user_base.join("Programs/Real")).unwrap();
        let menu_order =
            "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\MenuOrder";
        let mock = Mock::new()
            .subkey(menu_order, &["Start Menu"])
            .subkey(&format!(r"{menu_order}\Start Menu"), &["Programs"])
            .subkey(
                &format!(r"{menu_order}\Start Menu\Programs"),
                &["Real", "Gone"],
            );

        let issues = start_menu_ordering_issues(&mock, &user_base, &root.join("Common"));
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].key,
            format!(r"{menu_order}\Start Menu\Programs\Gone")
        );
        assert!(issues[0].value.is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn fix_reports_every_item_as_failed_where_no_registry_exists() {
        let outcome = fix(&[("HKEY_CURRENT_USER\\Software\\X".to_owned(), None)]);
        assert!(outcome.fixed.is_empty());
        assert_eq!(outcome.failed.len(), 1);
    }
}
