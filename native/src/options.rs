use crate::theme::default_tokens_path;
use std::path::PathBuf;

pub struct Options {
    pub theme: PathBuf,
}

pub const HELP: &str = "Relay native\n\n  relay-native                 Run the fleet console\n  relay-native --theme FILE    Read canonical tokens from FILE\n\nDefault tokens: $AI_HOME/config/theme/tokens.json, otherwise ~/.ai/config/theme/tokens.json.";

fn value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .ok_or_else(|| format!("{option} requires a value"))
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut theme = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--theme" if theme.is_none() => {
                    theme = Some(PathBuf::from(value(&mut args, &arg)?))
                }
                _ => return Err(format!("Unknown or repeated option: {arg}\n{HELP}")),
            }
        }
        Ok(Self {
            theme: match theme {
                Some(path) => path,
                None => default_tokens_path()?,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_arguments_not_shell_commands() {
        let options =
            Options::parse(["--theme", "/tmp/theme with spaces/tokens.json"].map(str::to_owned))
                .unwrap();
        assert_eq!(
            options.theme,
            PathBuf::from("/tmp/theme with spaces/tokens.json")
        );
        assert!(Options::parse(["--theme".into()]).is_err());
        assert!(Options::parse(["--unknown".into()]).is_err());
        assert!(Options::parse(["--gallery".into()]).is_err());
    }
}
