use crate::args::{Cli, command_with_version_banner};
use clap::{ArgAction, FromArgMatches};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) fn parse(
    arguments: impl IntoIterator<Item = OsString>,
    search_paths: impl FnOnce() -> std::io::Result<Vec<PathBuf>>,
) -> Result<Cli, clap::Error> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let mut command = command_with_version_banner();
    let matches = command.clone().try_get_matches_from(&arguments)?;
    if matches.get_flag("no_config") {
        return Cli::from_arg_matches(&matches);
    }
    let paths = search_paths()
        .map_err(|error| clap::Error::raw(clap::error::ErrorKind::Io, error.to_string()))?;
    for path in paths {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(config_error(&path, error)),
        };
        let config: Value =
            serde_json::from_str(&text).map_err(|error| config_error(&path, error))?;
        apply_defaults(&mut command, &config).map_err(|error| config_error(&path, error))?;
        let matches = command.try_get_matches_from(&arguments)?;
        return Cli::from_arg_matches(&matches);
    }
    Cli::from_arg_matches(&matches)
}

fn config_error(path: &Path, error: impl std::fmt::Display) -> clap::Error {
    clap::Error::raw(
        clap::error::ErrorKind::ValueValidation,
        format!("cannot read configuration {}: {error}", path.display()),
    )
}

fn apply_defaults(command: &mut clap::Command, config: &Value) -> Result<(), String> {
    let sections = config
        .as_object()
        .ok_or("expected an object of command sections")?;
    for (name, settings) in sections {
        let subcommand = command
            .find_subcommand_mut(name)
            .filter(|subcommand| subcommand.get_name() == name)
            .ok_or_else(|| format!("unknown command section '{name}'"))?;
        let settings = settings
            .as_object()
            .ok_or_else(|| format!("section '{name}' must be an object"))?;
        for (key, value) in settings {
            let argument = subcommand
                .get_arguments()
                .find(|argument| {
                    argument.get_long() == Some(key.as_str()) && !argument.is_global_set()
                })
                .ok_or_else(|| format!("unknown option '{name}.{key}'"))?;
            let values = match argument.get_action() {
                ArgAction::SetTrue | ArgAction::SetFalse => vec![
                    value
                        .as_bool()
                        .ok_or_else(|| format!("'{name}.{key}' must be a boolean"))?
                        .to_string(),
                ],
                ArgAction::Count => vec![
                    value
                        .as_u64()
                        .filter(|count| *count <= u8::MAX.into())
                        .ok_or_else(|| format!("'{name}.{key}' must be an integer from 0 to 255"))?
                        .to_string(),
                ],
                ArgAction::Set => vec![
                    scalar(value)
                        .ok_or_else(|| format!("'{name}.{key}' must be a string or number"))?,
                ],
                ArgAction::Append => value
                    .as_array()
                    .ok_or_else(|| format!("'{name}.{key}' must be an array"))?
                    .iter()
                    .map(|value| {
                        scalar(value).ok_or_else(|| {
                            format!("'{name}.{key}' entries must be strings or numbers")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(format!("option '{name}.{key}' cannot be configured")),
            };
            let id = argument.get_id().clone();
            *subcommand = subcommand
                .clone()
                .mut_arg(id, |argument| argument.default_values(values));
        }
    }
    Ok(())
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Command;

    fn parse_at(arguments: &[&str], paths: &[PathBuf]) -> Result<Cli, clap::Error> {
        parse(arguments.iter().map(OsString::from), || Ok(paths.to_vec()))
    }

    #[test]
    fn config_defaults_are_overridden_by_cli_and_work_with_command_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("zarc.json");
        std::fs::write(&path, r#"{"create":{"compression-level":7,"metadata":true,"include-file":["*.rs","*.txt"]},"info":{"verbose":2}}"#).unwrap();
        let cli = parse_at(&["zarc", "c", "a.zar", "input"], &[path.clone()]).unwrap();
        assert!(
            matches!(cli.command, Command::Create { compression_level: Some(level), metadata: true, include_files, .. } if level == "7" && include_files == ["*.rs", "*.txt"])
        );
        let cli = parse_at(
            &[
                "zarc",
                "create",
                "a.zar",
                "input",
                "--compression-level",
                "3",
                "--include-file",
                "*.c",
            ],
            &[path.clone()],
        )
        .unwrap();
        assert!(
            matches!(cli.command, Command::Create { compression_level: Some(level), include_files, .. } if level == "3" && include_files == ["*.c"])
        );
        let cli = parse_at(&["zarc", "info", "a.zar"], &[path.clone()]).unwrap();
        assert!(matches!(cli.command, Command::Info { verbose: 2, .. }));
        let cli = parse_at(&["zarc", "info", "a.zar", "-v"], &[path]).unwrap();
        assert!(matches!(cli.command, Command::Info { verbose: 1, .. }));
    }

    #[test]
    fn search_uses_first_file_and_falls_back_to_user_config_only_if_missing() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("local.json");
        let user = directory.path().join("user.json");
        std::fs::write(&user, r#"{"create":{"metadata":true}}"#).unwrap();
        let paths = [local.clone(), user];
        let cli = parse_at(&["zarc", "create", "a.zar", "input"], &paths).unwrap();
        assert!(matches!(
            cli.command,
            Command::Create { metadata: true, .. }
        ));
        std::fs::write(&local, "{}").unwrap();
        let cli = parse_at(&["zarc", "create", "a.zar", "input"], &paths).unwrap();
        assert!(matches!(
            cli.command,
            Command::Create {
                metadata: false,
                ..
            }
        ));
        std::fs::write(&local, "invalid").unwrap();
        assert!(parse_at(&["zarc", "create", "a.zar", "input"], &paths).is_err());
    }

    #[test]
    fn no_config_and_help_skip_configuration_discovery() {
        for arguments in [
            vec!["zarc", "--no-config", "create", "a.zar", "input"],
            vec!["zarc", "create", "a.zar", "input", "--no-config"],
        ] {
            let cli = parse(arguments.iter().map(OsString::from), || {
                panic!("must not search")
            })
            .unwrap();
            assert!(matches!(
                cli.command,
                Command::Create {
                    metadata: false,
                    ..
                }
            ));
        }
        let error = parse(["zarc", "--help"].map(OsString::from), || {
            panic!("must not search")
        })
        .unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn rejects_invalid_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("zarc.json");
        for text in [
            "[]",
            r#"{"unknown":{}}"#,
            r#"{"create":[]}"#,
            r#"{"create":{"unknown":true}}"#,
            r#"{"create":{"metadata":"yes"}}"#,
            r#"{"create":{"include-file":"*.rs"}}"#,
            r#"{"create":{"archive":"a.zar"}}"#,
            r#"{"create":{"aead":"invalid"}}"#,
            r#"{"info":{"verbose":256}}"#,
        ] {
            std::fs::write(&path, text).unwrap();
            assert!(
                parse_at(&["zarc", "create", "a.zar", "input"], &[path.clone()]).is_err(),
                "{text}"
            );
        }
    }
}
