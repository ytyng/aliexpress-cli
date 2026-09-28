//! THIRD-PARTY-NOTICES.txt has to follow the dependencies. Adding or upgrading
//! one without running `scripts/generate-third-party-notices.sh` fails here.

use std::process::Command;

use aliexpress_cli::THIRD_PARTY_NOTICES;

const LOCK: &str = include_str!("../../../Cargo.lock");
const CLI_MANIFEST: &str = include_str!("../Cargo.toml");
const CORE_MANIFEST: &str = include_str!("../../aliexpress-core/Cargo.toml");

/// Windows checkouts may turn line endings into CRLF (autocrlf).
fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The dependencies a manifest compiles into the binary: the `[dependencies]`
/// and `[target.*.dependencies]` sections, one `name = ...` or
/// `name.workspace = true` per line. Build and dev dependencies do not ship.
/// The workspace's own crates are not third-party and are skipped.
fn direct_dependencies(manifest: &str) -> Vec<String> {
    let mut section = String::new();
    let mut names = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.to_string();
            continue;
        }
        let shipped = section == "[dependencies]"
            || (section.starts_with("[target.") && section.ends_with(".dependencies]"));
        if !shipped || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            let name = key.trim().trim_end_matches(".workspace");
            if !name.starts_with("aliexpress-") {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// The (name, version) pairs under "Used by:" in the notices. An entry is a
/// separator, `License: ...`, a blank line and "Used by:"; only that sequence
/// opens a list, so the same words inside a license text are not counted.
fn packages_in_notices(notices: &str) -> Vec<(String, String)> {
    let separator = "=".repeat(80);
    let mut packages = Vec::new();
    let mut in_block = false;
    let mut after_separator = false;
    let mut after_license = false;
    for line in notices.lines() {
        if in_block {
            let mut words = line.strip_prefix("  ").unwrap_or("").split(' ');
            match (words.next(), words.next()) {
                (Some(name), Some(version)) if !name.is_empty() => {
                    packages.push((name.to_string(), version.to_string()));
                }
                _ => in_block = false,
            }
            continue;
        }
        in_block = after_license && line == "Used by:";
        after_license = (after_separator && line.starts_with("License: "))
            || (after_license && line.is_empty());
        after_separator = line == separator;
    }
    packages
}

/// The `[[package]]` blocks of Cargo.lock: (name, version, dependency lines).
fn locked_packages(lock: &str) -> Vec<(String, String, Vec<String>)> {
    lock.split("[[package]]")
        .skip(1)
        .map(|block| {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|line| line.strip_prefix(key))
                    .map(|rest| rest.trim().trim_matches('"').to_string())
                    .unwrap_or_default()
            };
            let deps = block
                .lines()
                .filter_map(|line| line.strip_prefix(" \""))
                .map(|line| line.trim_end_matches("\",").to_string())
                .collect();
            (field("name = "), field("version = "), deps)
        })
        .collect()
}

/// The version Cargo.lock picked for `name` as a dependency of the workspace
/// crate `owner`. A crate locked at more than one version is written
/// `name version` in the owner's dependency list.
fn resolved_version(lock: &[(String, String, Vec<String>)], owner: &str, name: &str) -> String {
    let (_, _, deps) = lock
        .iter()
        .find(|(crate_name, _, _)| crate_name == owner)
        .unwrap_or_else(|| panic!("Cargo.lock has no {owner}"));
    let entry = deps
        .iter()
        .find(|dep| *dep == name || dep.starts_with(&format!("{name} ")))
        .unwrap_or_else(|| panic!("{name} is not a dependency of {owner} in Cargo.lock"));
    match entry.split(' ').nth(1) {
        Some(version) => version.to_string(),
        None => {
            let mut versions = lock
                .iter()
                .filter(|(crate_name, _, _)| crate_name == name)
                .map(|(_, version, _)| version.clone());
            let version = versions.next().expect("the crate is in Cargo.lock");
            assert!(
                versions.next().is_none(),
                "{name} has several versions in Cargo.lock"
            );
            version
        }
    }
}

/// Every direct dependency is listed, at the version Cargo.lock picked. By
/// name alone an upgraded crate would pass while its old version lingers as a
/// transitive dependency.
#[test]
fn every_direct_dependency_is_listed() {
    // Arrange
    let lock = locked_packages(&lf(LOCK));
    let listed = packages_in_notices(&lf(THIRD_PARTY_NOTICES));
    assert!(listed.len() > 100, "parsed notices: {listed:?}");
    let mut wanted = Vec::new();
    for (owner, manifest) in [
        ("aliexpress-cli", CLI_MANIFEST),
        ("aliexpress-core", CORE_MANIFEST),
    ] {
        for name in direct_dependencies(&lf(manifest)) {
            let version = resolved_version(&lock, owner, &name);
            wanted.push((name, version));
        }
    }
    assert!(
        wanted.iter().any(|(name, _)| name == "tauri")
            && wanted.iter().any(|(name, _)| name == "ureq"),
        "parsed dependencies: {wanted:?}"
    );
    // Act
    let missing: Vec<&(String, String)> = wanted
        .iter()
        .filter(|entry| !listed.contains(entry))
        .collect();
    // Assert
    assert!(
        missing.is_empty(),
        "not in THIRD-PARTY-NOTICES.txt (run scripts/generate-third-party-notices.sh): {missing:?}"
    );
}

/// A listed crate at a version Cargo.lock no longer has means a dependency moved
/// and the notices were not regenerated.
#[test]
fn every_listed_crate_is_in_cargo_lock() {
    // Arrange
    let lock = locked_packages(&lf(LOCK));
    let listed = packages_in_notices(&lf(THIRD_PARTY_NOTICES));
    assert!(listed.len() > 100, "parsed notices: {listed:?}");
    // Act
    let stale: Vec<&(String, String)> = listed
        .iter()
        .filter(|(name, version)| !lock.iter().any(|(n, v, _)| n == name && v == version))
        .collect();
    // Assert
    assert!(
        stale.is_empty(),
        "not in Cargo.lock (run scripts/generate-third-party-notices.sh): {stale:?}"
    );
}

/// The workspace's own crates are not third-party.
#[test]
fn own_crates_are_not_listed() {
    // Arrange
    let listed = packages_in_notices(&lf(THIRD_PARTY_NOTICES));
    // Act
    let own: Vec<&(String, String)> = listed
        .iter()
        .filter(|(name, _)| name.starts_with("aliexpress-"))
        .collect();
    // Assert
    assert!(own.is_empty(), "{own:?}");
}

#[test]
fn parsers_accept_crlf() {
    // Arrange
    let notices = "x\r\n\r\n".to_string()
        + &"=".repeat(80)
        + "\r\nLicense: MIT\r\n\r\nUsed by:\r\n  serde 1.0.0 (u)\r\n\r\ntext\r\n";
    let manifest = "[dependencies]\r\nserde.workspace = true\r\nureq = \"3\"\r\n\r\n[build-dependencies]\r\ncc = \"1\"\r\n";
    // Act
    let listed = packages_in_notices(&lf(&notices));
    let deps = direct_dependencies(&lf(manifest));
    // Assert
    assert_eq!(listed, vec![("serde".to_string(), "1.0.0".to_string())]);
    assert_eq!(deps, vec!["serde".to_string(), "ureq".to_string()]);
}

/// `--license` prints this tool's license and the notices, and exits 0 without
/// a keyword.
#[test]
fn license_flag_prints_the_notices() {
    // Arrange
    let binary = env!("CARGO_BIN_EXE_aliexpress");
    // Act
    let output = Command::new(binary)
        .arg("--license")
        .output()
        .expect("the binary runs");
    // Assert
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.starts_with("MIT License"), "{}", &stdout[..200]);
    assert!(stdout.contains("THIRD-PARTY NOTICES"));
    assert!(stdout.contains("# Rust crates"));
}
