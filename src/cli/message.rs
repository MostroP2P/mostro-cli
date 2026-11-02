//! Dispatch of `message list`: read-only. Configuration, identity and the
//! existing store are resolved in order of cost before any relay is contacted;
//! nothing is written and nothing is asked.

use std::io::{self, Write};

use chrono::Utc;

use super::commands::ListMessagesArgs;
use super::invocation::OutputMode;
use super::migrated::{code_name, diagnose};
use super::write::resolve_reader;
use super::Cli;
use crate::commands::message::list::{message_list, ListOutcome, COMMAND};
use crate::msg::ReadLimits;
use crate::outcome::messages::MessageListInput;
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings, RUNTIME_ERROR_EXIT};
use crate::parse_human::render_messages;
use crate::parse_machine::emit;

const UNWRITABLE: &str = "error: the messages could not be written";

/// Run `message list` and return its exit status.
pub(super) async fn list(cli: &Cli, args: &ListMessagesArgs, output: OutputMode) -> i32 {
    let outcome = outcome(cli, args).await;
    let exit = match output {
        OutputMode::Json => emit(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
        OutputMode::Human => print(&outcome, &mut io::stdout().lock(), &mut io::stderr().lock()),
    };
    if cli.verbose {
        let result = outcome
            .error_code()
            .map_or_else(|| "ok".to_string(), code_name);
        diagnose(
            &mut io::stderr().lock(),
            &format!("verbose: {COMMAND} {result}"),
        );
    }
    exit
}

async fn outcome(cli: &Cli, args: &ListMessagesArgs) -> ListOutcome {
    let Ok(input) = MessageListInput::validate(args.since, args.order) else {
        return refused(ErrorCode::InvalidArguments);
    };
    let reader = match resolve_reader(cli, false).await {
        Ok(reader) => reader,
        Err(code) => return refused(code),
    };
    message_list(
        &input,
        &reader.config,
        &reader.store,
        &ReadLimits::default(),
        Utc::now(),
    )
    .await
}

/// A failure before any relay was contacted.
fn refused(code: ErrorCode) -> ListOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::read(None),
    )
}

/// The human result on `out`, or a fixed error line on `err`. Rendered
/// completely before the first byte is written.
fn print(outcome: &ListOutcome, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let Some(data) = outcome.data() else {
        let message = outcome
            .error_code()
            .unwrap_or(ErrorCode::InternalError)
            .message();
        diagnose(err, &format!("error: {message}"));
        return outcome.exit_code();
    };
    match out
        .write_all(render_messages(data).as_bytes())
        .and_then(|()| out.flush())
    {
        Ok(()) => outcome.exit_code(),
        Err(_) => {
            diagnose(err, UNWRITABLE);
            RUNTIME_ERROR_EXIT
        }
    }
}
