//! Human presentation of `config.set` and `config.show`: the config file's
//! path, then each setting's stored value or `not set`.

use std::fmt::Write;

use super::table::escape_controls;
use crate::outcome::config::ConfigData;
use crate::settings::Setting;

/// What the path line says when the data has no path.
const NO_PATH: &str = "unknown (no data directory, or its path is not UTF-8)";

/// The config file and the values stored in it.
pub fn render_config(data: &ConfigData) -> String {
    let mut out = String::new();
    let path = data
        .path()
        .map_or_else(|| NO_PATH.to_owned(), escape_controls);
    let _ = writeln!(out, "Config file: {path}");
    for setting in Setting::ALL {
        let value = data.value(setting).unwrap_or("not set");
        let _ = writeln!(out, "{}: {value}", setting.name());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ConfigFile;
    use std::path::Path;

    #[test]
    fn render_shows_the_path_and_each_value_or_not_set() {
        let file = ConfigFile::parse("RELAYS=wss://a\nTRANSPORT=nip44\n").unwrap();
        let path = Path::new("/tmp/a\u{1b}b/config");
        assert_eq!(
            render_config(&ConfigData::new(Some(path), Some(&file))),
            "Config file: /tmp/a\\u{1b}b/config\nMOSTRO_PUBKEY: not set\nRELAYS: wss://a\n\
             POW: not set\nTRANSPORT: nip44\n"
        );
        let nowhere = render_config(&ConfigData::new(None, None));
        // A path that is not UTF-8 is null too, so the line names both cases.
        let first = "Config file: unknown (no data directory, or its path is not UTF-8)\n";
        assert!(
            nowhere.starts_with(&format!("{first}MOSTRO_PUBKEY: not set\n")),
            "{nowhere}"
        );
    }
}
