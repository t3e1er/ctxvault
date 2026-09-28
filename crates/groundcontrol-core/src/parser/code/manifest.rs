//! Package manifest parser and scoper for in-process SCIP moniker synthesis.
//!
//! Rapidly (<1ms) extracts package manager schemes, package names, and versions
//! from `Cargo.toml`, `go.mod`, `package.json`, `pom.xml`, `build.gradle`, and
//! `pyproject.toml` to bound symbols to compiler-grade namespaces.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Extracted package manifest metadata for SCIP moniker formatting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    /// SCIP scheme, e.g. "scip-rust", "scip-go", "scip-typescript", "scip-java", "scip-python".
    pub scheme: String,
    /// Package manager, e.g. "cargo", "gomod", "npm", "maven", "pip".
    pub manager: String,
    /// Package or module namespace name.
    pub package_name: String,
    /// Semantic version string (defaults to "0.0.0" if unspecified).
    pub version: String,
    /// Root directory path of the manifest.
    pub root_dir: String,
}

impl PackageManifest {
    /// Create a new package manifest record.
    pub fn new(
        scheme: impl Into<String>,
        manager: impl Into<String>,
        package_name: impl Into<String>,
        version: impl Into<String>,
        root_dir: impl Into<String>,
    ) -> Self {
        Self {
            scheme: scheme.into(),
            manager: manager.into(),
            package_name: package_name.into(),
            version: version.into(),
            root_dir: root_dir.into(),
        }
    }
}

/// Parse a `Cargo.toml` file into a [`PackageManifest`].
pub fn parse_cargo_toml(content: &str, dir: &Path) -> Option<PackageManifest> {
    let val: toml::Value = toml::from_str(content).ok()?;
    let dir_str = dir.to_string_lossy().replace('\\', "/");

    if let Some(pkg) = val.get("package") {
        let name = pkg.get("name").and_then(|v| v.as_str())?;
        let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        return Some(PackageManifest::new("scip-rust", "cargo", name, version, dir_str));
    }

    // Workspace root fallback
    if val.get("workspace").is_some() {
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("workspace");
        return Some(PackageManifest::new("scip-rust", "cargo", dir_name, "0.0.0", dir_str));
    }

    None
}

/// Parse a `go.mod` file into a [`PackageManifest`].
pub fn parse_go_mod(content: &str, dir: &Path) -> Option<PackageManifest> {
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            let module_path = rest.split_whitespace().next()?.trim();
            if !module_path.is_empty() {
                return Some(PackageManifest::new(
                    "scip-go",
                    "gomod",
                    module_path,
                    "0.0.0",
                    dir_str,
                ));
            }
        }
    }
    None
}

/// Parse a `package.json` file into a [`PackageManifest`].
pub fn parse_package_json(content: &str, dir: &Path) -> Option<PackageManifest> {
    let val: serde_json::Value = serde_json::from_str(content).ok()?;
    let dir_str = dir.to_string_lossy().replace('\\', "/");

    let fallback_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("package");
    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or(fallback_name);
    let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");

    Some(PackageManifest::new("scip-typescript", "npm", name, version, dir_str))
}

/// Parse a Maven `pom.xml` file into a [`PackageManifest`].
pub fn parse_pom_xml(content: &str, dir: &Path) -> Option<PackageManifest> {
    let dir_str = dir.to_string_lossy().replace('\\', "/");

    // Fast-path robust XML tag extraction for groupId, artifactId, and version
    let extract_tag = |tag: &str| -> Option<String> {
        let open_tag = format!("<{}>", tag);
        let close_tag = format!("</{}>", tag);
        let start = content.find(&open_tag)? + open_tag.len();
        let end = content[start..].find(&close_tag)? + start;
        let val = content[start..end].trim();
        if val.is_empty() {
            None
        } else {
            Some(val.to_string())
        }
    };

    let artifact_id = extract_tag("artifactId")?;
    let group_id = extract_tag("groupId");
    let version = extract_tag("version").unwrap_or_else(|| "0.0.0".to_string());

    let package_name =
        if let Some(gid) = group_id { format!("{gid}.{artifact_id}") } else { artifact_id };

    Some(PackageManifest::new("scip-java", "maven", package_name, version, dir_str))
}

/// Parse a Gradle build file (`build.gradle` / `build.gradle.kts`) into a [`PackageManifest`].
pub fn parse_build_gradle(content: &str, dir: &Path) -> Option<PackageManifest> {
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    let fallback_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("app");

    // Look for group and version assignments
    let mut group: Option<String> = None;
    let mut version = "0.0.0".to_string();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("group") {
            if let Some(val) = extract_quoted_value(trimmed) {
                group = Some(val);
            }
        } else if trimmed.starts_with("version") {
            if let Some(val) = extract_quoted_value(trimmed) {
                version = val;
            }
        }
    }

    let package_name = if let Some(g) = group {
        format!("{g}.{fallback_name}")
    } else {
        fallback_name.to_string()
    };

    Some(PackageManifest::new("scip-java", "gradle", package_name, version, dir_str))
}

/// Parse a `pyproject.toml` file into a [`PackageManifest`].
pub fn parse_pyproject_toml(content: &str, dir: &Path) -> Option<PackageManifest> {
    let val: toml::Value = toml::from_str(content).ok()?;
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    let fallback_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("project");

    if let Some(proj) = val.get("project") {
        let name = proj.get("name").and_then(|v| v.as_str()).unwrap_or(fallback_name);
        let version = proj.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        return Some(PackageManifest::new("scip-python", "pip", name, version, dir_str));
    }

    if let Some(poetry) = val.get("tool").and_then(|t| t.get("poetry")) {
        let name = poetry.get("name").and_then(|v| v.as_str()).unwrap_or(fallback_name);
        let version = poetry.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        return Some(PackageManifest::new("scip-python", "pip", name, version, dir_str));
    }

    Some(PackageManifest::new("scip-python", "pip", fallback_name, "0.0.0", dir_str))
}

/// Parse a `setup.py` file into a [`PackageManifest`].
pub fn parse_setup_py(content: &str, dir: &Path) -> Option<PackageManifest> {
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    let fallback_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("project");

    let mut name = fallback_name.to_string();
    let mut version = "0.0.0".to_string();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") {
            if let Some(val) = extract_quoted_value(trimmed) {
                name = val;
            }
        } else if trimmed.starts_with("version") {
            if let Some(val) = extract_quoted_value(trimmed) {
                version = val;
            }
        }
    }

    Some(PackageManifest::new("scip-python", "pip", name, version, dir_str))
}

fn extract_quoted_value(line: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(first) = line.find(quote) {
            if let Some(second) = line[first + 1..].find(quote) {
                let inner = &line[first + 1..first + 1 + second];
                return Some(inner.to_string());
            }
        }
    }
    None
}

/// Detect a package manifest in the given directory.
pub fn detect_manifest_in_dir(dir: &Path) -> Option<PackageManifest> {
    // 1. Cargo.toml
    let cargo = dir.join("Cargo.toml");
    if cargo.is_file() {
        if let Ok(content) = std::fs::read_to_string(&cargo) {
            if let Some(m) = parse_cargo_toml(&content, dir) {
                return Some(m);
            }
        }
    }

    // 2. go.mod
    let go_mod = dir.join("go.mod");
    if go_mod.is_file() {
        if let Ok(content) = std::fs::read_to_string(&go_mod) {
            if let Some(m) = parse_go_mod(&content, dir) {
                return Some(m);
            }
        }
    }

    // 3. package.json
    let pkg_json = dir.join("package.json");
    if pkg_json.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Some(m) = parse_package_json(&content, dir) {
                return Some(m);
            }
        }
    }

    // 4. pom.xml
    let pom = dir.join("pom.xml");
    if pom.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pom) {
            if let Some(m) = parse_pom_xml(&content, dir) {
                return Some(m);
            }
        }
    }

    // 5. build.gradle / build.gradle.kts
    let gradle = dir.join("build.gradle");
    let gradle_kts = dir.join("build.gradle.kts");
    let gradle_file = if gradle.is_file() {
        Some(gradle)
    } else if gradle_kts.is_file() {
        Some(gradle_kts)
    } else {
        None
    };
    if let Some(g) = gradle_file {
        if let Ok(content) = std::fs::read_to_string(g) {
            if let Some(m) = parse_build_gradle(&content, dir) {
                return Some(m);
            }
        }
    }

    // 6. pyproject.toml
    let pyproj = dir.join("pyproject.toml");
    if pyproj.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pyproj) {
            if let Some(m) = parse_pyproject_toml(&content, dir) {
                return Some(m);
            }
        }
    }

    // 7. setup.py
    let setup = dir.join("setup.py");
    if setup.is_file() {
        if let Ok(content) = std::fs::read_to_string(&setup) {
            if let Some(m) = parse_setup_py(&content, dir) {
                return Some(m);
            }
        }
    }

    None
}

/// Walk up from `file_or_dir` towards filesystem root (or `root_limit`), returning
/// the nearest enclosing [`PackageManifest`].
pub fn find_enclosing_manifest(
    file_or_dir: &Path,
    root_limit: Option<&Path>,
) -> Option<PackageManifest> {
    let mut current: PathBuf = if file_or_dir.is_file() {
        file_or_dir.parent()?.to_path_buf()
    } else {
        file_or_dir.to_path_buf()
    };

    let limit_canonical = root_limit.and_then(|l| l.canonicalize().ok());

    loop {
        if let Some(m) = detect_manifest_in_dir(&current) {
            return Some(m);
        }

        if let Some(ref lim) = limit_canonical {
            if let Ok(curr_canon) = current.canonicalize() {
                if &curr_canon == lim {
                    break;
                }
            }
        }

        if !current.pop() {
            break;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_toml() {
        let content = r#"
[package]
name = "groundcontrol-core"
version = "0.2.2"
edition = "2021"
"#;
        let dir = Path::new("/workspace/crates/groundcontrol-core");
        let manifest = parse_cargo_toml(content, dir).unwrap();
        assert_eq!(manifest.scheme, "scip-rust");
        assert_eq!(manifest.manager, "cargo");
        assert_eq!(manifest.package_name, "groundcontrol-core");
        assert_eq!(manifest.version, "0.2.2");
    }

    #[test]
    fn test_parse_go_mod() {
        let content = r#"
module github.com/open-telemetry/opentelemetry-demo/src/checkoutservice

go 1.22
"#;
        let dir = Path::new("/workspace/src/checkoutservice");
        let manifest = parse_go_mod(content, dir).unwrap();
        assert_eq!(manifest.scheme, "scip-go");
        assert_eq!(manifest.manager, "gomod");
        assert_eq!(
            manifest.package_name,
            "github.com/open-telemetry/opentelemetry-demo/src/checkoutservice"
        );
        assert_eq!(manifest.version, "0.0.0");
    }

    #[test]
    fn test_parse_package_json() {
        let content = r#"
{
  "name": "frontend",
  "version": "1.0.0",
  "dependencies": {}
}
"#;
        let dir = Path::new("/workspace/src/frontend");
        let manifest = parse_package_json(content, dir).unwrap();
        assert_eq!(manifest.scheme, "scip-typescript");
        assert_eq!(manifest.manager, "npm");
        assert_eq!(manifest.package_name, "frontend");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_parse_pom_xml() {
        let content = r#"
<project>
    <groupId>com.oteldemo</groupId>
    <artifactId>adservice</artifactId>
    <version>0.1.0</version>
</project>
"#;
        let dir = Path::new("/workspace/src/adservice");
        let manifest = parse_pom_xml(content, dir).unwrap();
        assert_eq!(manifest.scheme, "scip-java");
        assert_eq!(manifest.manager, "maven");
        assert_eq!(manifest.package_name, "com.oteldemo.adservice");
        assert_eq!(manifest.version, "0.1.0");
    }

    #[test]
    fn test_parse_pyproject_toml() {
        let content = r#"
[project]
name = "paymentservice"
version = "0.1.0"
"#;
        let dir = Path::new("/workspace/src/paymentservice");
        let manifest = parse_pyproject_toml(content, dir).unwrap();
        assert_eq!(manifest.scheme, "scip-python");
        assert_eq!(manifest.manager, "pip");
        assert_eq!(manifest.package_name, "paymentservice");
        assert_eq!(manifest.version, "0.1.0");
    }
}
