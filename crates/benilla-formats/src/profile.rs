//! Explicit data dialect selection. Archive discovery never guesses the client version.
use anyhow::{bail, Result};

/// A client data dialect and its archive locale.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DataProfile {
    #[default]
    Vanilla,
    Wrath {
        locale: String,
    },
}

impl DataProfile {
    /// Parse launcher configuration; absent profile preserves the vanilla default.
    pub fn parse(profile: Option<&str>, locale: Option<&str>) -> Result<Self> {
        match profile.unwrap_or("vanilla") {
            "vanilla" => Ok(Self::Vanilla),
            "wrath" => {
                let locale = locale.unwrap_or("enUS");
                if !matches!(
                    locale,
                    "enUS"
                        | "enGB"
                        | "koKR"
                        | "frFR"
                        | "deDE"
                        | "zhCN"
                        | "zhTW"
                        | "esES"
                        | "esMX"
                        | "ruRU"
                ) {
                    bail!("unsupported Wrath locale {locale:?}");
                }
                Ok(Self::Wrath {
                    locale: locale.into(),
                })
            }
            other => bail!("unsupported WOW_CLIENT_PROFILE {other:?}; expected vanilla or wrath"),
        }
    }

    /// Read the explicit launcher switches. Invalid values fail rather than falling back.
    pub fn from_env() -> Result<Self> {
        let profile = std::env::var("WOW_CLIENT_PROFILE");
        let locale = std::env::var("WOW_LOCALE");
        let read =
            |value: std::result::Result<String, std::env::VarError>| -> Result<Option<String>> {
                match value {
                    Ok(value) => Ok(Some(value)),
                    Err(std::env::VarError::NotPresent) => Ok(None),
                    Err(error) => Err(error.into()),
                }
            };
        Self::parse(read(profile)?.as_deref(), read(locale)?.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_dialects_and_locales() {
        assert_eq!(
            DataProfile::parse(None, None).unwrap(),
            DataProfile::Vanilla
        );
        assert_eq!(
            DataProfile::parse(Some("wrath"), None).unwrap(),
            DataProfile::Wrath {
                locale: "enUS".into()
            }
        );
        assert!(DataProfile::parse(Some("12340"), None).is_err());
        assert!(DataProfile::parse(Some("wrath"), Some("../x")).is_err());
        assert!(DataProfile::parse(Some("wrath"), Some("frFR")).is_ok());
        assert!(DataProfile::parse(Some("wrath"), Some("enGB")).is_ok());
        assert!(DataProfile::parse(Some("wrath"), Some("xxxx")).is_err());
        assert!(DataProfile::parse(Some("wrath"), Some("enus")).is_err());
    }
}
