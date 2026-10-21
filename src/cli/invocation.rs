use clap::error::ErrorKind;
use clap::{Arg, ArgMatches, Command, ValueEnum};
use std::ffi::OsString;
use std::str::from_utf8;

/// Long name of the global output option.
pub const OUTPUT_LONG: &str = "output";
/// Exit code for every argument error, whatever the output mode.
pub const ARGUMENT_ERROR_EXIT: i32 = 2;
/// Stable error code of a JSON argument-error envelope.
pub const INVALID_ARGUMENTS: &str = "invalid_arguments";

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputMode {
    Human,
    Json,
}

/// Output selection found in argv.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputSelection {
    /// No declaration (human by default) or exactly one valid declaration.
    Mode(OutputMode),
    /// A missing or invalid value, or more than one declaration.
    Invalid,
}

#[derive(Debug)]
pub enum Invocation {
    /// Clap accepted argv. `output` is the declared mode or the human default.
    Parsed {
        matches: ArgMatches,
        output: OutputMode,
    },
    /// Clap displayed help or version: print it as text and exit 0, even when
    /// JSON was selected.
    Text(clap::Error),
    /// Argument error: exit [`ARGUMENT_ERROR_EXIT`] without running anything.
    Rejected(ArgumentFailure),
}

/// What an argument error report may say. It deliberately carries no Clap
/// message, because those echo raw argument values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentFailure {
    pub output: OutputSelection,
    /// Leaf command path such as `["order", "list"]`; `None` when unresolved.
    pub command: Option<Vec<String>>,
}

impl ArgumentFailure {
    /// Error code for a JSON envelope; `None` means empty stdout and a safe
    /// stderr diagnostic instead.
    pub fn json_error_code(&self) -> Option<&'static str> {
        (self.output == OutputSelection::Mode(OutputMode::Json)).then_some(INVALID_ARGUMENTS)
    }
}

/// Facts recovered from argv by following `command`'s Clap metadata.
struct Scan {
    output: OutputSelection,
    command: Option<Vec<String>>,
}

/// Parse `args` (including the binary name) with Clap, and on an argument
/// error recover the output selection and leaf command without executing
/// anything.
pub fn parse_invocation<I, T>(mut command: Command, args: I) -> Invocation
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    command.build();
    let scan = scan(&command, args.get(1..).unwrap_or_default());
    match command.try_get_matches_from_mut(&args) {
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Invocation::Text(error)
        }
        Ok(matches) => match scan.output {
            OutputSelection::Mode(output) => Invocation::Parsed { matches, output },
            OutputSelection::Invalid => Invocation::Rejected(ArgumentFailure {
                output: OutputSelection::Invalid,
                command: scan.command,
            }),
        },
        Err(_) => Invocation::Rejected(ArgumentFailure {
            output: scan.output,
            command: scan.command,
        }),
    }
}

/// Walk argv the way Clap would: options take values per their arity and
/// hyphen settings, subcommand names descend only while every earlier one
/// resolved, and `--` ends option parsing. Output declarations count only in
/// option positions, so value text is never searched. After an unknown
/// command name, later tokens keep the last resolved scope.
fn scan(root: &Command, args: &[OsString]) -> Scan {
    let mut scope = root;
    let mut path = Vec::new();
    let mut resolved = true;
    let mut declarations: Vec<Option<&[u8]>> = Vec::new();
    let mut next = 0;
    while let Some(arg) = args.get(next) {
        let token = arg.as_encoded_bytes();
        next += 1;
        if token == b"--" {
            break;
        }
        if let Some(long) = token.strip_prefix(b"--") {
            let (name, attached) = match long.iter().position(|&byte| byte == b'=') {
                Some(at) => (&long[..at], Some(&long[at + 1..])),
                None => (long, None),
            };
            let Some(option) = text(name).and_then(|name| find_long(scope, name)) else {
                continue;
            };
            let value = match attached {
                Some(value) => Some(value),
                None => take_values(option, args, &mut next),
            };
            if option.get_long() == Some(OUTPUT_LONG) {
                declarations.push(value);
            }
        } else if token.len() > 1 && token[0] == b'-' {
            take_short_cluster(scope, &token[1..], args, &mut next);
        } else if resolved && scope.has_subcommands() {
            match text(token).and_then(|name| scope.find_subcommand(name)) {
                Some(subcommand) => {
                    path.push(subcommand.get_name().to_string());
                    scope = subcommand;
                }
                None => resolved = false,
            }
        }
    }
    let output = match declarations.as_slice() {
        [] => OutputSelection::Mode(OutputMode::Human),
        [Some(value)] if *value == b"json" => OutputSelection::Mode(OutputMode::Json),
        [Some(value)] if *value == b"human" => OutputSelection::Mode(OutputMode::Human),
        _ => OutputSelection::Invalid,
    };
    let leaf = resolved && !path.is_empty() && !scope.has_subcommands();
    Scan {
        output,
        command: leaf.then_some(path),
    }
}

fn text(bytes: &[u8]) -> Option<&str> {
    from_utf8(bytes).ok()
}

fn find_long<'a>(scope: &'a Command, name: &str) -> Option<&'a Arg> {
    scope.get_arguments().find(|arg| {
        arg.get_long() == Some(name)
            || arg
                .get_all_aliases()
                .is_some_and(|aliases| aliases.contains(&name))
    })
}

fn find_short(scope: &Command, short: char) -> Option<&Arg> {
    scope.get_arguments().find(|arg| {
        arg.get_short() == Some(short)
            || arg
                .get_all_short_aliases()
                .is_some_and(|aliases| aliases.contains(&short))
    })
}

/// Handle `-abc`: flags until one takes a value, which then owns the rest of
/// the cluster (minus one leading `=`) or, if nothing is attached, the
/// following tokens. An unknown short stops the cluster, as Clap's error does.
fn take_short_cluster(scope: &Command, cluster: &[u8], args: &[OsString], next: &mut usize) {
    let readable = match from_utf8(cluster) {
        Ok(readable) => readable,
        Err(error) => text(&cluster[..error.valid_up_to()]).unwrap_or_default(),
    };
    for (at, short) in readable.char_indices() {
        let Some(option) = find_short(scope, short) else {
            return;
        };
        if takes_values(option) {
            if at + short.len_utf8() == cluster.len() {
                take_values(option, args, next);
            }
            return;
        }
    }
}

fn takes_values(option: &Arg) -> bool {
    option
        .get_num_args()
        .is_some_and(|range| range.takes_values())
}

/// Consume the separate values Clap binds to `option`, returning the first.
fn take_values<'a>(option: &Arg, args: &'a [OsString], next: &mut usize) -> Option<&'a [u8]> {
    let max = match option.get_num_args() {
        Some(range) if range.takes_values() => range.max_values(),
        _ => return None,
    };
    let mut first = None;
    let mut taken = 0;
    while taken < max {
        let Some(candidate) = args.get(*next) else {
            break;
        };
        let bytes = candidate.as_encoded_bytes();
        if bytes == b"--"
            || (!option.is_allow_hyphen_values_set() && looks_like_flag(option, bytes))
        {
            break;
        }
        first.get_or_insert(bytes);
        taken += 1;
        *next += 1;
    }
    first
}

fn looks_like_flag(option: &Arg, bytes: &[u8]) -> bool {
    let Some(rest) = bytes.strip_prefix(b"-").filter(|rest| !rest.is_empty()) else {
        return false;
    };
    !(option.is_allow_negative_numbers_set()
        && text(rest).is_some_and(|number| number.parse::<f64>().is_ok()))
}
