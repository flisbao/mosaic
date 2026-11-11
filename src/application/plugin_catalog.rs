use crate::domain::plugin_package::{GitHubSource, PluginPackageError};

pub const OFFICIAL_PLUGINS: [&str; 6] = [
    "azure-devops",
    "azure-pipelines",
    "calendar",
    "github",
    "gitlab",
    "jira",
];

pub fn resolve_plugin_source(value: &str) -> Result<GitHubSource, PluginPackageError> {
    if OFFICIAL_PLUGINS.contains(&value) {
        GitHubSource::parse(&format!("flisbao/mosaic/plugins/{value}"))
    } else {
        GitHubSource::parse(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_catalog_is_complete_and_sorted() {
        assert_eq!(
            OFFICIAL_PLUGINS,
            [
                "azure-devops",
                "azure-pipelines",
                "calendar",
                "github",
                "gitlab",
                "jira"
            ]
        );
        for name in OFFICIAL_PLUGINS {
            assert_eq!(
                resolve_plugin_source(name).unwrap().reference(),
                format!("flisbao/mosaic/plugins/{name}")
            );
        }
        assert_eq!(
            resolve_plugin_source("owner/repo/plugin")
                .unwrap()
                .reference(),
            "owner/repo/plugin"
        );
        assert!(resolve_plugin_source("unknown").is_err());
    }
}
