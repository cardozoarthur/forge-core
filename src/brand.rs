use std::borrow::Cow;
use std::env::VarError;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const PRODUCT_NAME: &str = "Foundry Core";
pub const PACKAGE_NAME: &str = "foundry-core";
pub const LIBRARY_NAME: &str = "foundry_core";
pub const PRIMARY_BINARY_NAME: &str = "foundry";
pub const DEFAULT_STATE_DIRECTORY: &str = ".foundry";
pub const DEFAULT_STORE_FILE: &str = "foundry.sqlite";

const CANONICAL_ENV_PREFIX: &str = "FOUNDRY_";
const LEGACY_ENV_PREFIX: &str = "FORGE_"; // foundry-brand-allow: legacy-compat
const CANONICAL_IDENTIFIER_PREFIX: &str = "foundry.";
const LEGACY_IDENTIFIER_PREFIX: &str = "forge."; // foundry-brand-allow: legacy-compat
const CANONICAL_URI_PREFIX: &str = "foundry://";
const LEGACY_URI_PREFIX: &str = "forge://"; // foundry-brand-allow: legacy-compat
pub(crate) const LEGACY_STATE_DIRECTORY: &str = ".forge"; // foundry-brand-allow: legacy-compat
const LEGACY_AUTHORITY_ID: &str = "forge"; // foundry-brand-allow: legacy-compat
const LEGACY_CLI_AUTHORITY_ID: &str = "forge_cli"; // foundry-brand-allow: legacy-compat
const LEGACY_CONTROL_PLANE_ID: &str = "forge-control-plane"; // foundry-brand-allow: legacy-compat
const LEGACY_ORIGINAL_ORIGIN: &str = "forge-original"; // foundry-brand-allow: legacy-compat

/// Reads a process environment variable with Foundry-first precedence.
///
/// Calls using either the canonical or legacy product prefix resolve the
// foundry-brand-allow: legacy-compat
/// canonical `FOUNDRY_*` value first, then fall back to its `FORGE_*` spelling
/// for the 0.6.x compatibility cycle. Unrelated environment variables retain
/// the behavior of `std::env::var`.
pub fn env_var(name: impl AsRef<str>) -> Result<String, VarError> {
    let name = name.as_ref();
    let Some((canonical, legacy)) = product_environment_names(name) else {
        return std::env::var(name);
    };
    match std::env::var(&canonical) {
        Ok(value) => Ok(value),
        Err(VarError::NotPresent) => std::env::var(legacy),
        Err(error) => Err(error),
    }
}

/// `env_var` equivalent for non-Unicode environment values.
pub fn env_var_os(name: impl AsRef<OsStr>) -> Option<OsString> {
    let name = name.as_ref();
    let Some(name) = name.to_str() else {
        return std::env::var_os(name);
    };
    let Some((canonical, legacy)) = product_environment_names(name) else {
        return std::env::var_os(name);
    };
    std::env::var_os(canonical).or_else(|| std::env::var_os(legacy))
}

fn product_environment_names(name: &str) -> Option<(String, String)> {
    if let Some(suffix) = name.strip_prefix(CANONICAL_ENV_PREFIX) {
        return Some((name.to_string(), format!("{LEGACY_ENV_PREFIX}{suffix}")));
    }
    name.strip_prefix(LEGACY_ENV_PREFIX)
        .map(|suffix| (format!("{CANONICAL_ENV_PREFIX}{suffix}"), name.to_string()))
}

/// Converts a legacy schema, tool or event identifier into its canonical form.
pub fn canonical_identifier(value: &str) -> Cow<'_, str> {
    value
        .strip_prefix(LEGACY_IDENTIFIER_PREFIX)
        .map(|suffix| Cow::Owned(format!("{CANONICAL_IDENTIFIER_PREFIX}{suffix}")))
        .unwrap_or_else(|| Cow::Borrowed(value))
}

/// Converts a legacy Foundry URI into its canonical form.
pub fn canonical_uri(value: &str) -> Cow<'_, str> {
    value
        .strip_prefix(LEGACY_URI_PREFIX)
        .map(|suffix| Cow::Owned(format!("{CANONICAL_URI_PREFIX}{suffix}")))
        .unwrap_or_else(|| Cow::Borrowed(value))
}

/// Converts exact persisted authority, provider, owner and provenance ids into
/// their canonical Foundry spelling without rewriting the source record.
pub fn canonical_authority(value: &str) -> Cow<'_, str> {
    match value {
        LEGACY_AUTHORITY_ID => Cow::Borrowed("foundry"),
        LEGACY_CLI_AUTHORITY_ID => Cow::Borrowed("foundry_cli"),
        LEGACY_CONTROL_PLANE_ID => Cow::Borrowed("foundry-control-plane"),
        LEGACY_ORIGINAL_ORIGIN => Cow::Borrowed("foundry-original"),
        _ => Cow::Borrowed(value),
    }
}

/// Compares a persisted or inbound identifier against a canonical Foundry id.
pub fn identifier_matches(value: &str, canonical: &str) -> bool {
    canonical_identifier(value).as_ref() == canonical
}

/// JSON-value form used by compatibility validation at typed input boundaries.
pub fn json_identifier_matches(value: &serde_json::Value, canonical: &str) -> bool {
    value
        .as_str()
        .is_some_and(|value| identifier_matches(value, canonical))
}

/// Resolves a project configuration for reading without moving or rewriting it.
/// Canonical state wins by presence; the legacy directory is consulted only
/// when the canonical file does not exist.
pub fn project_config_path_for_read(project_root: &Path, file_name: &str) -> PathBuf {
    let canonical = project_root.join(DEFAULT_STATE_DIRECTORY).join(file_name);
    if canonical.exists() {
        return canonical;
    }
    let legacy = project_root.join(LEGACY_STATE_DIRECTORY).join(file_name);
    if legacy.exists() {
        return legacy;
    }
    canonical
}

/// Multi-extension variant that preserves product precedence before extension
/// preference (all canonical candidates are checked before any legacy one).
pub fn find_project_config_for_read(project_root: &Path, file_names: &[&str]) -> Option<PathBuf> {
    file_names
        .iter()
        .map(|file_name| project_root.join(DEFAULT_STATE_DIRECTORY).join(file_name))
        .find(|path| path.is_file())
        .or_else(|| {
            file_names
                .iter()
                .map(|file_name| project_root.join(LEGACY_STATE_DIRECTORY).join(file_name))
                .find(|path| path.is_file())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_legacy_identifiers_and_uris() {
        assert_eq!(
            canonical_identifier("forge.workflow.inspect"), // foundry-brand-allow: legacy-compat
            "foundry.workflow.inspect"
        );
        assert_eq!(
            canonical_uri("forge://artifact/example"), // foundry-brand-allow: legacy-compat
            "foundry://artifact/example"
        );
        assert_eq!(
            canonical_authority("forge"), // foundry-brand-allow: legacy-compat
            "foundry"
        );
        assert_eq!(
            canonical_authority("forge_cli"), // foundry-brand-allow: legacy-compat
            "foundry_cli"
        );
    }
}
