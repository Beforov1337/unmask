use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn get_arch() -> String {
    if let Ok(arch) = env::var("ARCH") {
        if !arch.trim().is_empty() {
            return arch.trim().to_string();
        }
    }
    if let Ok(out) = Command::new("portageq").args(["envvar", "ARCH"]).output() {
        let arch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !arch.is_empty() {
            return arch;
        }
    }
    #[cfg(target_arch = "x86_64")]
    return "amd64".to_string();
    #[cfg(target_arch = "aarch64")]
    return "arm64".to_string();
    #[cfg(target_arch = "x86")]
    return "x86".to_string();
    #[allow(unreachable_code)]
    "amd64".to_string()
}

fn get_repos() -> Vec<PathBuf> {
    let mut repos = Vec::new();
    if let Ok(out) = Command::new("portageq").args(["repos_path", "/"]).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let path = PathBuf::from(line.trim());
            if path.is_dir() && !repos.contains(&path) {
                repos.push(path);
            }
        }
    }
    let default_repos = ["/var/db/repos/gentoo", "/var/db/repos/guru"];
    for &d in &default_repos {
        let p = PathBuf::from(d);
        if p.is_dir() && !repos.contains(&p) {
            repos.push(p);
        }
    }
    repos
}

struct MatchResult {
    full_atom: String,      // e.g. "gui-apps/quickshell"
    pkg_name: String,       // e.g. "quickshell"
    has_live: bool,         // has ** (e.g. 9999 / missing keyword)
}

fn scan_package(target: &str) -> Option<MatchResult> {
    let repos = get_repos();
    let is_category_pkg = target.contains('/');

    let mut found_category_pkg: Option<String> = None;
    let mut actual_pkg_name = String::new();
    let mut has_live = false;

    let target_norm = target.trim();

    for repo in &repos {
        if let Ok(entries) = fs::read_dir(repo) {
            for cat_entry in entries.flatten() {
                let cat_name = cat_entry.file_name().to_string_lossy().to_string();
                if cat_name.starts_with('.') || cat_name == "metadata" || cat_name == "profiles" || cat_name == "eclass" {
                    continue;
                }
                let cat_path = cat_entry.path();
                if !cat_path.is_dir() {
                    continue;
                }

                if let Ok(pkg_entries) = fs::read_dir(&cat_path) {
                    for pkg_entry in pkg_entries.flatten() {
                        let cur_pkg_name = pkg_entry.file_name().to_string_lossy().to_string();
                        let cur_full = format!("{}/{}", cat_name, cur_pkg_name);

                        let matches = if is_category_pkg {
                            cur_full == target_norm
                        } else {
                            cur_pkg_name == target_norm
                        };

                        if matches {
                            if found_category_pkg.is_none() {
                                found_category_pkg = Some(cur_full);
                                actual_pkg_name = cur_pkg_name;
                            }

                            if let Ok(ebuild_files) = fs::read_dir(pkg_entry.path()) {
                                for ef in ebuild_files.flatten() {
                                    let fname = ef.file_name().to_string_lossy().to_string();
                                    if fname.ends_with(".ebuild") {
                                        if fname.contains("9999") {
                                            has_live = true;
                                        }
                                        if let Ok(content) = fs::read_to_string(ef.path()) {
                                            for line in content.lines() {
                                                if line.trim_start().starts_with("KEYWORDS=") {
                                                    let kw = line;
                                                    if kw.contains("-*") || kw.contains("~*") || kw.contains("**") {
                                                        has_live = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if found_category_pkg.is_some() {
            break;
        }
    }

    found_category_pkg.map(|full_atom| MatchResult {
        full_atom,
        pkg_name: actual_pkg_name,
        has_live,
    })
}

fn append_entry_if_missing(file_path: &Path, entry: &str) -> io::Result<bool> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut existing = HashSet::new();
    if file_path.exists() {
        let content = fs::read_to_string(file_path)?;
        for line in content.lines() {
            let l = line.trim();
            if !l.is_empty() && !l.starts_with('#') {
                existing.insert(l.to_string());
            }
        }
    }

    if existing.contains(entry) {
        return Ok(false);
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;

    writeln!(file, "{}", entry)?;
    Ok(true)
}

fn get_target_file(base_dir: &str, clean_name: &str) -> PathBuf {
    let dir = Path::new(base_dir);
    if dir.is_dir() {
        dir.join(clean_name)
    } else {
        PathBuf::from(base_dir)
    }
}

// Parses emerge --autounmask output to unmask dependencies and USE flags
fn resolve_masked_dependencies(pkg: &str, clean_name: &str) {
    let output = Command::new("emerge")
        .args(["--autounmask", "--autounmask-write", "-p", pkg])
        .output();

    let out = match output {
        Ok(o) => o,
        Err(_) => return,
    };

    let full_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    #[derive(PartialEq)]
    enum Section {
        None,
        Keywords,
        Use,
        Unmask,
    }

    let mut current_sec = Section::None;

    let keywords_file = get_target_file("/etc/portage/package.accept_keywords", clean_name);
    let use_file = get_target_file("/etc/portage/package.use", clean_name);
    let unmask_file = get_target_file("/etc/portage/package.unmask", clean_name);

    for line in full_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("The following keyword changes are necessary") {
            current_sec = Section::Keywords;
            continue;
        } else if trimmed.starts_with("The following USE changes are necessary") {
            current_sec = Section::Use;
            continue;
        } else if trimmed.starts_with("The following mask changes are necessary") {
            current_sec = Section::Unmask;
            continue;
        } else if trimmed.starts_with("Calculating")
            || trimmed.starts_with("Dependency resolution")
            || trimmed.starts_with("These are the packages")
            || trimmed.starts_with("[ebuild")
        {
            current_sec = Section::None;
            continue;
        }

        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        match current_sec {
            Section::Keywords => {
                let _ = append_entry_if_missing(&keywords_file, trimmed);
            }
            Section::Use => {
                let _ = append_entry_if_missing(&use_file, trimmed);
            }
            Section::Unmask => {
                let _ = append_entry_if_missing(&unmask_file, trimmed);
            }
            Section::None => {}
        }
    }

    // Auto-merge any config protect files created by emerge if etc-update is present
    let _ = Command::new("etc-update")
        .args(["--automode", "-5", "/etc/portage"])
        .output();
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Использование: unmask <имя_пакета>");
        eprintln!("Пример: unmask quickshell  или  unmask gui-apps/quickshell");
        return ExitCode::FAILURE;
    }

    let target = &args[1];
    let arch = get_arch();

    let pkg_info = match scan_package(target) {
        Some(info) => info,
        None => {
            eprintln!("Ошибка: Пакет '{}' не найден в доступных репозиториях portage.", target);
            return ExitCode::FAILURE;
        }
    };

    let clean_name = pkg_info.pkg_name.replace('/', "_");

    let unmask_file = get_target_file("/etc/portage/package.unmask", &clean_name);
    let keywords_file = get_target_file("/etc/portage/package.accept_keywords", &clean_name);

    let kw_entry = format!("{} ~{}", pkg_info.full_atom, arch);
    let live_entry = format!("{} **", pkg_info.full_atom);

    // 1. Write direct target to package.accept_keywords
    if let Err(e) = append_entry_if_missing(&keywords_file, &kw_entry) {
        eprintln!("[-] Ошибка записи в {}: {}. Запустите через doas/sudo!", keywords_file.display(), e);
        return ExitCode::FAILURE;
    }

    if pkg_info.has_live {
        if let Err(e) = append_entry_if_missing(&keywords_file, &live_entry) {
            eprintln!("[-] Ошибка записи в {}: {}. Запустите через doas/sudo!", keywords_file.display(), e);
            return ExitCode::FAILURE;
        }
    }

    // 2. Write direct target to package.unmask
    if let Err(e) = append_entry_if_missing(&unmask_file, &pkg_info.full_atom) {
        eprintln!("[-] Ошибка записи в {}: {}. Запустите через doas/sudo!", unmask_file.display(), e);
        return ExitCode::FAILURE;
    }

    // 3. Resolve all masked dependencies and required USE flags recursively
    resolve_masked_dependencies(&pkg_info.full_atom, &clean_name);

    println!("unmask successful");

    ExitCode::SUCCESS
}
