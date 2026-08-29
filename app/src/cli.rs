//! The command surface of `quotadeckctl`.
//!
//! Parsing lives here rather than in the binary because the argument surface is the contract a
//! script depends on, and a contract that can only be exercised by running a process is a
//! contract nobody tests. Everything below is pure: it turns words into a [`Command`] or into
//! the sentence explaining why it could not, and it reads no file and touches no setting.
//!
//! Two rules the shape enforces:
//!
//! - **A missing half is never invented.** `--from` without `--to` is refused rather than
//!   completed with "now", because a range a caller did not ask for is a wrong answer that
//!   looks like a right one.
//! - **A conflict is a conflict.** `--json --csv` is an error rather than last-one-wins; a
//!   script that accidentally passes both is asking for two different files.

use chrono::{DateTime, Utc};

use crate::export::ExportFormat;

/// What the caller asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    /// Every compiled provider, its confidence level and whether it is installed here.
    Providers,
    /// Parse and print one provider, or every enabled one.
    Status {
        provider: Option<String>,
        plan: Option<String>,
    },
    Export(ExportArgs),
    /// The resolved settings, as they are stored.
    ConfigShow,
    /// Whether the stored settings can be resolved against this build's registry.
    ConfigValidate,
    /// Where this process thinks it is: home, data directory and per-root access. The sandbox
    /// regression harness reads exactly these lines.
    Guard,
    Tray {
        provider: String,
    },
    Statusline(StatuslineAction),
}

/// The Claude Code status line shim. The two mutating verbs are spelled out rather than
/// defaulted into, and both refuse to run inside the App Store sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatuslineAction {
    Preview,
    Install,
    Revert,
}

/// The export request as words, before it is resolved against the stored settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArgs {
    pub format: Option<ExportFormat>,
    pub provider: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

/// Why the arguments were refused. One sentence, written for a terminal, sent to stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub message: String,
}

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        CliError {
            message: message.into(),
        }
    }
}

pub const MISSING_COMMAND: &str = "a command is required; run `quotadeckctl --help`";
pub const PLAN_NEEDS_PROVIDER: &str =
    "--plan applies to one provider; pass --provider with it, because a plan is not shared between tools";
pub const CONFLICTING_FORMAT: &str = "--json and --csv cannot both be given";
pub const FROM_NEEDS_TO: &str = "--from requires the paired --to flag";
pub const TO_NEEDS_FROM: &str = "--to requires the paired --from flag";

pub const HELP: &str = "\
quotadeckctl - read this machine's AI quota from a terminal

usage:
  quotadeckctl <command> [options]

commands:
  providers                       compiled providers, their confidence level and roots
  status [--provider <key>] [--plan <id>]
                                  parse the logs and print what each window reports
  export [--json|--csv] [--provider <key>] [--from <RFC3339> --to <RFC3339>]
                                  the deck to stdout; exits 0 ok, 10 near, 11 hit, 20 unknown
  config show                     the stored settings, as they are on disk
  config validate                 resolve the stored settings against this build's registry
  guard                           resolved home, data directory and per-root access
  tray <key>                      draw the menu bar item for that provider
  statusline preview              what connecting the Claude Code status line would change
  statusline install              write the shim into settings.json
  statusline revert               put settings.json back

options:
  -h, --help                      this text
  -V, --version                   the version this binary was built from

Every command reads local files only. Data goes to stdout, diagnostics to stderr.";

/// Turn the words after the program name into a [`Command`].
pub fn parse(args: &[String]) -> Result<Command, CliError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::new(MISSING_COMMAND));
    };
    let rest = &args[1..];
    match command {
        "--help" | "-h" | "help" => {
            reject_extra(rest, "help")?;
            Ok(Command::Help)
        }
        "--version" | "-V" => {
            reject_extra(rest, "version")?;
            Ok(Command::Version)
        }
        "providers" => {
            reject_extra(rest, "providers")?;
            Ok(Command::Providers)
        }
        "guard" => {
            reject_extra(rest, "guard")?;
            Ok(Command::Guard)
        }
        "status" => parse_status(rest),
        "export" => parse_export(rest).map(Command::Export),
        "config" => match subcommand(rest, "config", &["show", "validate"])? {
            ("show", rest) => {
                reject_extra(rest, "config show")?;
                Ok(Command::ConfigShow)
            }
            (_, rest) => {
                reject_extra(rest, "config validate")?;
                Ok(Command::ConfigValidate)
            }
        },
        "statusline" => {
            let (verb, rest) = subcommand(rest, "statusline", &["preview", "install", "revert"])?;
            reject_extra(rest, "statusline")?;
            Ok(Command::Statusline(match verb {
                "preview" => StatuslineAction::Preview,
                "install" => StatuslineAction::Install,
                _ => StatuslineAction::Revert,
            }))
        }
        "tray" => {
            let (provider, rest) = value_after(rest, "tray")?;
            reject_extra(rest, "tray")?;
            Ok(Command::Tray { provider })
        }
        other => Err(CliError::new(format!(
            "unknown command: {other}; run `quotadeckctl --help`"
        ))),
    }
}

fn parse_status(args: &[String]) -> Result<Command, CliError> {
    let mut provider = None;
    let mut plan = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--provider" => provider = Some(flag_value(&mut rest, "--provider")?),
            "--plan" => plan = Some(flag_value(&mut rest, "--plan")?),
            other => return Err(unknown_option("status", other)),
        }
    }
    if plan.is_some() && provider.is_none() {
        return Err(CliError::new(PLAN_NEEDS_PROVIDER));
    }
    Ok(Command::Status { provider, plan })
}

fn parse_export(args: &[String]) -> Result<ExportArgs, CliError> {
    let mut format: Option<ExportFormat> = None;
    let mut provider = None;
    let mut from = None;
    let mut to = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let chosen = match arg.as_str() {
            "--json" => Some(ExportFormat::Json),
            "--csv" => Some(ExportFormat::Csv),
            "--provider" => {
                provider = Some(flag_value(&mut rest, "--provider")?);
                None
            }
            "--from" => {
                from = Some(parse_rfc3339("--from", &flag_value(&mut rest, "--from")?)?);
                None
            }
            "--to" => {
                to = Some(parse_rfc3339("--to", &flag_value(&mut rest, "--to")?)?);
                None
            }
            other => return Err(unknown_option("export", other)),
        };
        if let Some(chosen) = chosen {
            if format.is_some_and(|earlier| earlier != chosen) {
                return Err(CliError::new(CONFLICTING_FORMAT));
            }
            format = Some(chosen);
        }
    }
    match (from.is_some(), to.is_some()) {
        (true, false) => return Err(CliError::new(FROM_NEEDS_TO)),
        (false, true) => return Err(CliError::new(TO_NEEDS_FROM)),
        _ => {}
    }
    Ok(ExportArgs {
        format,
        provider,
        from,
        to,
    })
}

/// An RFC3339 instant, timezone included. A bare date is refused rather than assumed to be UTC:
/// the caller's own midnight is the one they meant, and this process cannot know which it is.
fn parse_rfc3339(flag: &str, value: &str) -> Result<DateTime<Utc>, CliError> {
    DateTime::parse_from_rfc3339(value)
        .map(|instant| instant.with_timezone(&Utc))
        .map_err(|error| {
            CliError::new(format!(
                "{flag} must be an RFC3339 instant with a timezone; received {value:?}: {error}"
            ))
        })
}

fn flag_value<'a>(
    rest: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, CliError> {
    rest.next()
        .cloned()
        .ok_or_else(|| CliError::new(format!("{flag} needs a value")))
}

fn value_after<'a>(args: &'a [String], command: &str) -> Result<(String, &'a [String]), CliError> {
    match args.split_first() {
        Some((value, rest)) if !value.starts_with('-') => Ok((value.clone(), rest)),
        Some((value, _)) => Err(unknown_option(command, value)),
        None => Err(CliError::new(format!(
            "{command} needs a provider key; run `quotadeckctl providers` to see them"
        ))),
    }
}

fn subcommand<'a>(
    args: &'a [String],
    command: &str,
    accepted: &[&'static str],
) -> Result<(&'static str, &'a [String]), CliError> {
    let list = accepted.join(", ");
    let Some((word, rest)) = args.split_first() else {
        return Err(CliError::new(format!("{command} needs one of: {list}")));
    };
    accepted
        .iter()
        .find(|candidate| *candidate == word)
        .map(|candidate| (*candidate, rest))
        .ok_or_else(|| {
            CliError::new(format!(
                "unknown {command} command: {word}; expected {list}"
            ))
        })
}

fn reject_extra(rest: &[String], command: &str) -> Result<(), CliError> {
    match rest.first() {
        None => Ok(()),
        Some(extra) => Err(unknown_option(command, extra)),
    }
}

fn unknown_option(command: &str, argument: &str) -> CliError {
    CliError::new(format!(
        "unknown {command} argument: {argument}; run `quotadeckctl --help`"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn parsed(args: &[&str]) -> Command {
        parse(&args.iter().map(|a| (*a).to_string()).collect::<Vec<_>>())
            .expect("the arguments are accepted")
    }

    fn rejected(args: &[&str]) -> String {
        parse(&args.iter().map(|a| (*a).to_string()).collect::<Vec<_>>())
            .expect_err("the arguments are refused")
            .message
    }

    fn instant(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("a valid fixture instant")
            .with_timezone(&Utc)
    }

    #[test]
    fn no_arguments_ask_for_help_rather_than_guessing_a_command() {
        assert_eq!(rejected(&[]), MISSING_COMMAND);
    }

    #[test]
    fn help_and_version_are_answers_rather_than_errors() {
        assert_eq!(parsed(&["--help"]), Command::Help);
        assert_eq!(parsed(&["-h"]), Command::Help);
        assert_eq!(parsed(&["help"]), Command::Help);
        assert_eq!(parsed(&["--version"]), Command::Version);
        assert_eq!(parsed(&["-V"]), Command::Version);
    }

    #[test]
    fn every_documented_command_parses() {
        assert_eq!(parsed(&["providers"]), Command::Providers);
        assert_eq!(parsed(&["guard"]), Command::Guard);
        assert_eq!(parsed(&["config", "show"]), Command::ConfigShow);
        assert_eq!(parsed(&["config", "validate"]), Command::ConfigValidate);
        assert_eq!(
            parsed(&["statusline", "preview"]),
            Command::Statusline(StatuslineAction::Preview)
        );
        assert_eq!(
            parsed(&["statusline", "install"]),
            Command::Statusline(StatuslineAction::Install)
        );
        assert_eq!(
            parsed(&["statusline", "revert"]),
            Command::Statusline(StatuslineAction::Revert)
        );
        assert_eq!(
            parsed(&["tray", "codex"]),
            Command::Tray {
                provider: "codex".into()
            }
        );
    }

    #[test]
    fn status_reads_every_provider_unless_one_is_named() {
        assert_eq!(
            parsed(&["status"]),
            Command::Status {
                provider: None,
                plan: None
            }
        );
        assert_eq!(
            parsed(&["status", "--provider", "codex", "--plan", "max5"]),
            Command::Status {
                provider: Some("codex".into()),
                plan: Some("max5".into())
            }
        );
    }

    #[test]
    fn a_plan_without_a_provider_is_refused_because_it_would_apply_to_all_of_them() {
        assert_eq!(rejected(&["status", "--plan", "max5"]), PLAN_NEEDS_PROVIDER);
    }

    #[test]
    fn export_defaults_to_json_and_keeps_an_explicit_range() {
        assert_eq!(
            parsed(&["export"]),
            Command::Export(ExportArgs {
                format: None,
                provider: None,
                from: None,
                to: None
            })
        );
        assert_eq!(
            parsed(&[
                "export",
                "--csv",
                "--provider",
                "codex",
                "--from",
                "2026-08-01T00:00:00Z",
                "--to",
                "2026-08-02T00:00:00Z",
            ]),
            Command::Export(ExportArgs {
                format: Some(ExportFormat::Csv),
                provider: Some("codex".into()),
                from: Some(instant("2026-08-01T00:00:00Z")),
                to: Some(instant("2026-08-02T00:00:00Z")),
            })
        );
    }

    #[test]
    fn two_formats_are_a_conflict_rather_than_a_last_one_wins() {
        assert_eq!(rejected(&["export", "--json", "--csv"]), CONFLICTING_FORMAT);
        // The same flag twice says the same thing, so it is not a conflict.
        assert_eq!(
            parsed(&["export", "--csv", "--csv"]),
            Command::Export(ExportArgs {
                format: Some(ExportFormat::Csv),
                provider: None,
                from: None,
                to: None
            })
        );
    }

    #[test]
    fn half_a_range_is_refused_because_the_missing_half_would_be_invented() {
        assert_eq!(
            rejected(&["export", "--from", "2026-08-01T00:00:00Z"]),
            FROM_NEEDS_TO
        );
        assert_eq!(
            rejected(&["export", "--to", "2026-08-01T00:00:00Z"]),
            TO_NEEDS_FROM
        );
    }

    #[test]
    fn a_range_bound_must_carry_a_timezone() {
        let message = rejected(&["export", "--from", "2026-08-01", "--to", "2026-08-02"]);
        assert!(message.contains("--from"), "{message}");
        assert!(message.contains("RFC3339"), "{message}");
        assert!(message.contains("2026-08-01"), "{message}");
    }

    #[test]
    fn a_flag_that_takes_a_value_says_so_when_it_is_missing() {
        assert!(rejected(&["export", "--provider"]).contains("--provider"));
        assert!(rejected(&["export", "--from"]).contains("--from"));
        assert!(rejected(&["tray"]).contains("tray"));
    }

    #[test]
    fn an_unknown_word_names_itself_and_is_not_swallowed() {
        assert!(rejected(&["nonsense"]).contains("nonsense"));
        assert!(rejected(&["export", "--everything"]).contains("--everything"));
        assert!(rejected(&["config", "reset"]).contains("reset"));
        assert!(rejected(&["statusline", "uninstall"]).contains("uninstall"));
        assert!(rejected(&["providers", "codex"]).contains("codex"));
    }

    #[test]
    fn help_lists_every_command_it_accepts() {
        for command in [
            "providers",
            "status",
            "export",
            "config",
            "guard",
            "tray",
            "statusline",
        ] {
            assert!(HELP.contains(command), "help does not mention {command}");
        }
    }
}
