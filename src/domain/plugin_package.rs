use std::fmt;

use thiserror::Error;

const VERSION_COMPONENTS: usize = 3;
const MINIMUM_SOURCE_COMPONENTS: usize = 2;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PluginPackageError {
    #[error("fonte de plugin inválida: {0}")]
    InvalidSource(String),
    #[error("versão inválida: {0}; use MAJOR.MINOR.PATCH")]
    InvalidVersion(String),
    #[error("plugin exige Mosaic {required} ou mais recente; versão atual: {current}")]
    Incompatible { required: String, current: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PluginVersion([u64; VERSION_COMPONENTS]);

impl PluginVersion {
    pub fn parse(text: &str) -> Result<Self, PluginPackageError> {
        let invalid = || PluginPackageError::InvalidVersion(text.to_string());
        let parts: Vec<_> = text.split('.').collect();
        if parts.len() != VERSION_COMPONENTS {
            return Err(invalid());
        }
        let mut values = [0; VERSION_COMPONENTS];
        for (index, part) in parts.into_iter().enumerate() {
            if part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return Err(invalid());
            }
            values[index] = part.parse().map_err(|_| invalid())?;
        }
        Ok(Self(values))
    }
}

impl fmt::Display for PluginVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(formatter, "{major}.{minor}.{patch}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubSource {
    pub owner: String,
    pub repository: String,
    pub subdirectory: Vec<String>,
}

impl GitHubSource {
    pub fn parse(text: &str) -> Result<Self, PluginPackageError> {
        let parts: Vec<_> = text.split('/').collect();
        if parts.len() < MINIMUM_SOURCE_COMPONENTS
            || !parts.iter().all(|part| valid_source_component(part))
            || parts[1].ends_with(".git")
        {
            return Err(PluginPackageError::InvalidSource(text.to_string()));
        }
        Ok(Self {
            owner: parts[0].to_string(),
            repository: parts[1].to_string(),
            subdirectory: parts[MINIMUM_SOURCE_COMPONENTS..]
                .iter()
                .map(|part| part.to_string())
                .collect(),
        })
    }

    pub fn repository_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.repository)
    }

    pub fn reference(&self) -> String {
        let mut reference = format!("{}/{}", self.owner, self.repository);
        for component in &self.subdirectory {
            reference.push('/');
            reference.push_str(component);
        }
        reference
    }
}

fn valid_source_component(component: &str) -> bool {
    component
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageMetadata {
    pub version: PluginVersion,
    pub min_mosaic_version: PluginVersion,
}

impl PackageMetadata {
    pub fn ensure_compatible(&self, current: &PluginVersion) -> Result<(), PluginPackageError> {
        if current < &self.min_mosaic_version {
            return Err(PluginPackageError::Incompatible {
                required: self.min_mosaic_version.to_string(),
                current: current.to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically_and_round_trip() {
        assert!(PluginVersion::parse("0.0.10").unwrap() > PluginVersion::parse("0.0.9").unwrap());
        assert!(PluginVersion::parse("1.2.0").unwrap() < PluginVersion::parse("1.10.0").unwrap());
        assert_eq!(PluginVersion::parse("1.2.3").unwrap().to_string(), "1.2.3");
        for bad in [
            "",
            "1.2",
            "01.2.3",
            "1.2.3-beta",
            "+1.2.3",
            "1.2.3.4",
            "1.2.18446744073709551616",
        ] {
            assert!(PluginVersion::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn minimum_version_is_inclusive() {
        let metadata = PackageMetadata {
            version: PluginVersion::parse("0.0.4").unwrap(),
            min_mosaic_version: PluginVersion::parse("0.0.10").unwrap(),
        };
        assert!(metadata
            .ensure_compatible(&PluginVersion::parse("0.0.10").unwrap())
            .is_ok());
        assert!(metadata
            .ensure_compatible(&PluginVersion::parse("0.0.9").unwrap())
            .is_err());
    }

    #[test]
    fn sources_preserve_nested_paths_and_allow_safe_internal_punctuation() {
        let source = GitHubSource::parse("owner/repo/addons/review").unwrap();
        assert_eq!(source.repository_url(), "https://github.com/owner/repo.git");
        assert_eq!(source.reference(), "owner/repo/addons/review");
        assert_eq!(source.subdirectory, ["addons", "review"]);
        assert!(GitHubSource::parse("Owner/repo_name.v2/a-b").is_ok());
        assert!(GitHubSource::parse("owner/repo")
            .unwrap()
            .subdirectory
            .is_empty());
    }

    #[test]
    fn sources_reject_urls_escape_and_ambiguous_components() {
        for bad in [
            "owner",
            "../repo",
            "owner/repo/../x",
            "owner//repo",
            "owner/repo/",
            "https://github.com/a/b",
            "owner/repo.git",
            "owner/repo@v1",
            "owner/repo/%2e%2e",
            "owner/repo\\x",
            "/owner/repo",
            "owner/repo/a b",
            "owner/repo/á",
            "owner/repo/\u{1b}",
        ] {
            assert!(GitHubSource::parse(bad).is_err(), "{bad}");
        }
    }
}
