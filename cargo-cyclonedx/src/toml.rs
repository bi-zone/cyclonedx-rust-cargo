/*
 * This file is part of CycloneDX Rust Cargo.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 * SPDX-License-Identifier: Apache-2.0
 */
use crate::config::{self, CdxExtension, PrefixError};
use crate::config::{CustomPrefix, SbomConfig};
use crate::format::Format;

use serde::Deserialize;
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use thiserror::Error;

pub fn config_from_toml(value: Option<&toml::value::Value>) -> Result<SbomConfig, ConfigError> {
    if let Some(value) = value {
        let wrapper: ConfigWrapper = value
            .clone()
            .try_into()
            .map_err(|e| ConfigError::TomlError(format!("{}", e)))?;

        wrapper.try_into()
    } else {
        log::trace!("No Toml provided using default");
        Ok(SbomConfig::empty_config())
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ConfigWrapper {
    pub cyclonedx: Option<TomlConfig>,
}

impl TryFrom<ConfigWrapper> for SbomConfig {
    type Error = ConfigError;

    fn try_from(value: ConfigWrapper) -> Result<Self, Self::Error> {
        if let Some(cyclonedx) = value.cyclonedx {
            cyclonedx.try_into()
        } else {
            Ok(SbomConfig::empty_config())
        }
    }
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct TomlConfig {
    pub format: Option<Format>,
    pub included_dependencies: Option<IncludedDependencies>,
    pub output_options: Option<OutputOptions>,
}

impl TomlConfig {
    pub fn empty_config() -> Self {
        Self {
            format: None,
            included_dependencies: None,
            output_options: None,
        }
    }
}

impl TryFrom<TomlConfig> for SbomConfig {
    type Error = ConfigError;

    fn try_from(value: TomlConfig) -> Result<Self, Self::Error> {
        let output_options: Option<config::OutputOptions> = match value.output_options {
            Some(options) => Some(options.try_into()?),
            None => None,
        };

        Ok(Self {
            format: value.format,
            included_dependencies: value.included_dependencies.map(Into::into),
            output_options,
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum IncludedDependencies {
    #[serde(rename(deserialize = "top-level"))]
    TopLevelDependencies,
    #[serde(rename(deserialize = "all"))]
    AllDependencies,
}

impl Default for IncludedDependencies {
    fn default() -> Self {
        Self::TopLevelDependencies
    }
}

impl FromStr for IncludedDependencies {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(Self::AllDependencies),
            "top-level" => Ok(Self::TopLevelDependencies),
            _ => Err(format!("Expected all or top-level, got `{}`", s)),
        }
    }
}

impl From<IncludedDependencies> for config::IncludedDependencies {
    fn from(val: IncludedDependencies) -> Self {
        match val {
            IncludedDependencies::TopLevelDependencies => Self::TopLevelDependencies,
            IncludedDependencies::AllDependencies => Self::AllDependencies,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OutputOptions {
    #[serde(rename(deserialize = "cdx"))]
    pub cdx_extension: Option<bool>,
    #[serde(rename(deserialize = "pattern"))]
    pub pattern: Option<Pattern>,
    #[serde(rename(deserialize = "prefix"))]
    pub prefix: Option<String>,
}

impl TryFrom<OutputOptions> for config::OutputOptions {
    type Error = ConfigError;

    fn try_from(value: OutputOptions) -> Result<Self, Self::Error> {
        let cdx_extension = match value.cdx_extension {
            Some(true) => CdxExtension::Included,
            Some(false) => CdxExtension::NotIncluded,
            None => CdxExtension::default(),
        };

        let prefix = match (value.pattern, value.prefix) {
            (Some(pattern), None) => Ok(Some(config::Prefix::Pattern(pattern.into()))),
            (None, Some(prefix)) => {
                let prefix = CustomPrefix::new(prefix)?;
                Ok(Some(config::Prefix::Custom(prefix)))
            }
            (None, None) => Ok(None),
            _ => Err(ConfigError::ValidationError(
                "OutputOptions can contain either prefix or pattern, got both".to_string(),
            )),
        }?;
        Ok(Self {
            cdx_extension,
            prefix: prefix.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum Pattern {
    Bom,
    Package,
}

impl Default for Pattern {
    fn default() -> Self {
        Self::Bom
    }
}

impl FromStr for Pattern {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bom" => Ok(Self::Bom),
            "package" => Ok(Self::Package),
            _ => Err(format!("Expected bom or package, got `{}`", s)),
        }
    }
}

impl From<Pattern> for config::Pattern {
    fn from(val: Pattern) -> Self {
        match val {
            Pattern::Bom => Self::Bom,
            Pattern::Package => Self::Package,
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ConfigError {
    #[error("Failed to deserialize configuration from Toml: {0}")]
    TomlError(String),

    #[error("Failed to validate configuration: {0}")]
    ValidationError(String),

    #[error("Invalid prefix from Toml")]
    CustomPrefixError(#[from] PrefixError),
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_should_deserialize_from_toml_value() {
        let toml = r#"
[cyclonedx]
format = "json"
included_dependencies = "top-level"
output_options = { cdx = true, pattern = "bom", prefix = "tacos" }
"#;

        let actual: ConfigWrapper = toml::from_str(toml).expect("Failed to parse toml");

        let expected = TomlConfig {
            format: Some(Format::Json),
            included_dependencies: Some(IncludedDependencies::TopLevelDependencies),
            output_options: Some(OutputOptions {
                cdx_extension: Some(true),
                prefix: Some("tacos".to_string()),
                pattern: Some(Pattern::Bom),
            }),
        };

        assert_eq!(actual.cyclonedx, Some(expected));
    }

    #[test]
    fn it_should_return_an_error_for_mutually_exclusive_options() {
        let options = OutputOptions {
            cdx_extension: Some(true),
            pattern: Some(Pattern::Bom),
            prefix: Some("tacos".to_string()),
        };

        let actual: Result<config::OutputOptions, ConfigError> = options.try_into();

        let actual = actual
            .expect_err("Should not have been able to convert with mutually exclusive options");

        assert_eq!(
            actual,
            ConfigError::ValidationError(
                "OutputOptions can contain either prefix or pattern, got both".to_string()
            )
        );
    }

    #[test]
    fn it_should_convert_to_config_output_options() {
        let options = OutputOptions {
            cdx_extension: Some(true),
            pattern: Some(Pattern::Bom),
            prefix: None,
        };

        let actual: Result<config::OutputOptions, ConfigError> = options.try_into();

        let actual = actual.expect("Should have been able to convert to config::OutputOptions");

        let expected = config::OutputOptions {
            cdx_extension: config::CdxExtension::Included,
            prefix: config::Prefix::Pattern(config::Pattern::Bom),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn it_should_ignore_other_packages_from_toml_value() {
        let toml = r#"
[notourpackage]
format = "json"
included_dependencies = "top-level"
output_options = { cdx = true, pattern = "bom", prefix = "" }
"#;

        let actual: ConfigWrapper = toml::from_str(toml).expect("Failed to parse toml");

        assert_eq!(actual.cyclonedx, None);
    }
}
