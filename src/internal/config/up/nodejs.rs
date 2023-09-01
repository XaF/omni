use std::path::PathBuf;

use package_json::PackageJsonManager;
use semver::VersionReq;
use serde::{Deserialize, Serialize};

use crate::internal::config::up::UpConfigAsdfBase;
use crate::internal::config::up::UpError;
use crate::internal::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigNodejs {
    #[serde(skip)]
    pub asdf_base: UpConfigAsdfBase,
}

impl UpConfigNodejs {
    pub fn from_config_value(config_value: Option<&ConfigValue>) -> Self {
        let mut asdf_base = UpConfigAsdfBase::from_config_value("nodejs", config_value);
        asdf_base.add_detect_version_func(detect_version_from_package_json);

        Self {
            asdf_base: asdf_base,
        }
    }

    pub fn up(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base.up(progress)
    }

    pub fn down(&self, progress: Option<(usize, usize)>) -> Result<(), UpError> {
        self.asdf_base.down(progress)
    }
}

fn detect_version_from_package_json(_tool_name: String, path: PathBuf) -> Option<String> {
    if path
        .to_str()
        .unwrap()
        .to_string()
        .contains("/node_modules/")
    {
        return None;
    }

    let version_file_path = path.join("package.json");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    let mut manager = PackageJsonManager::with_file_path(version_file_path);

    let pkgfile = manager.read_ref();
    if pkgfile.is_err() {
        return None;
    }
    let pkgfile = pkgfile.unwrap();

    if pkgfile.engines.is_none() {
        return None;
    }
    let engines = pkgfile.engines.clone().unwrap();

    if let Some(node_version) = engines.get("node") {
        if let Ok(requirements) = VersionReq::parse(node_version) {
            for comparator in requirements.comparators {
                match comparator.op {
                    semver::Op::Exact
                    | semver::Op::Tilde
                    | semver::Op::Wildcard
                    | semver::Op::LessEq => {
                        let mut version = if let (Some(minor), Some(patch)) =
                            (comparator.minor, comparator.patch)
                        {
                            format!("{}.{}.{}", comparator.major, minor, patch)
                        } else if let Some(minor) = comparator.minor {
                            format!("{}.{}", comparator.major, minor)
                        } else {
                            format!("{}", comparator.major)
                        };

                        if comparator.pre != semver::Prerelease::EMPTY {
                            version = format!("{}-{}", version, comparator.pre.as_str());
                        }

                        return Some(version);
                    }
                    semver::Op::Caret => {
                        let major = comparator.major;
                        let mut minor = comparator.minor.unwrap_or(0);
                        let mut patch = comparator.patch.unwrap_or(0);

                        if major > 0 {
                            minor = 0;
                            patch = 0;
                        } else if minor > 0 {
                            patch = 0;
                        }

                        let parts = vec![major, minor, patch];
                        let version = parts
                            .iter()
                            .filter(|part| **part > 0)
                            .map(|part| part.to_string())
                            .collect::<Vec<String>>()
                            .join(".");

                        return Some(version);
                    }
                    semver::Op::Less => {
                        let version = if let (Some(minor), Some(patch)) =
                            (comparator.minor, comparator.patch)
                        {
                            format!("{}.{}.{}", comparator.major, minor, patch - 1)
                        } else if let Some(minor) = comparator.minor {
                            format!("{}.{}", comparator.major, minor - 1)
                        } else {
                            format!("{}", comparator.major - 1)
                        };

                        return Some(version);
                    }
                    semver::Op::Greater | semver::Op::GreaterEq => {
                        // Nothing to do, we can still install the latest
                    }
                    _ => {
                        unreachable!();
                    }
                }

                return Some("latest".to_string());
            }
        }
    }

    None
}
