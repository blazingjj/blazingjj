/*!
[Commander] member functions for reading and changing the jj
configuration.

The app only ever writes to the user's own config file, which is the
layer a setting belongs in when it says how the app is to look and
behave rather than anything about the repo.
*/
use anyhow::Context;
use anyhow::Result;
use tracing::instrument;

use crate::commander::Commander;

impl Commander {
    /// What the user's own config file says. Maps to
    /// `jj config list --user`.
    #[instrument(level = "trace", skip(self))]
    pub fn get_user_config(&self) -> Result<toml::Table> {
        // What jj lists is TOML of dotted keys.
        self.jj(["config", "list", "--user"])
            .run()
            .context("Failed to get jj config")?
            .parse()
            .context("Failed to parse jj config")
    }

    /// Set an option in the user's config file, `value` being the TOML
    /// expression to set it to. Maps to `jj config set --user`.
    #[instrument(level = "trace", skip(self))]
    pub fn set_user_config(&self, key: &str, value: &str) -> Result<()> {
        self.jj(["config", "set", "--user", key, value])
            .run_void()
            .context("Failed executing jj config set")
    }

    /// Take an option out of the user's config file, leaving whatever
    /// the other layers say. Maps to `jj config unset --user`.
    #[instrument(level = "trace", skip(self))]
    pub fn unset_user_config(&self, key: &str) -> Result<()> {
        self.jj(["config", "unset", "--user", key])
            .run_void()
            .context("Failed executing jj config unset")
    }
}

/// What the configuration says about `key`, which names a value through
/// the tables holding it, as `blazingjj.layout` does.
pub fn config_value<'a>(config: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let (path, name) = key.rsplit_once('.')?;

    let mut table = config;
    for step in path.split('.') {
        table = table.get(step)?.as_table()?;
    }

    table.get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> toml::Table {
        "blazingjj.layout = \"vertical\"\nui.diff.format = \"git\"\n"
            .parse()
            .expect("the listing parses")
    }

    #[test]
    fn a_dotted_key_names_a_value_through_the_tables_holding_it() {
        assert_eq!(
            config_value(&config(), "blazingjj.layout"),
            Some(&toml::Value::String("vertical".to_owned()))
        );
        assert_eq!(
            config_value(&config(), "ui.diff.format"),
            Some(&toml::Value::String("git".to_owned()))
        );
    }

    #[test]
    fn a_key_the_configuration_says_nothing_about_has_no_value() {
        assert_eq!(config_value(&config(), "blazingjj.layout-percent"), None);
        assert_eq!(config_value(&config(), "ui.diff.tool"), None);
        assert_eq!(config_value(&config(), "aliases.f.doc"), None);
    }
}
