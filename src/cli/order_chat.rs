//! Dispatch of `order chat`: the arguments first, then configuration, the
//! existing identity and the store in order of cost, before any relay is
//! contacted. Human mode sends `--message` without asking, as `dmtouser` does;
//! JSON never asks, so there `--message` without `--yes` is refused before
//! anything else. The work and transport settings and `--reply-timeout` are not
//! read: the message goes to relays, never to the node, mines nothing and waits
//! for no reply.

use chrono::Utc;

use super::commands::ChatArgs;
use super::invocation::OutputMode;
use super::write::{finish, needs_yes, resolve_reader};
use super::Cli;
use crate::commands::order::chat::{order_chat, ChatOutcome};
use crate::msg::ReadLimits;
use crate::outcome::chat::{ChatInput, COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings};
use crate::parse_human::render_chat;

const UNWRITABLE: &str = "error: the chat could not be written";
const ADVICE: &str =
    "The message may have reached a relay. Run `order chat --order <id>` and look \
                      for your message before sending it again.";

/// Run `order chat` and return its exit status.
pub(super) async fn run(cli: &Cli, args: &ChatArgs, output: OutputMode) -> i32 {
    let outcome = outcome(cli, args, output).await;
    finish(cli, output, &outcome, render_chat, (UNWRITABLE, ADVICE))
}

async fn outcome(cli: &Cli, args: &ChatArgs, output: OutputMode) -> ChatOutcome {
    if args.message.is_some() && needs_yes(output, args.yes) {
        return refused(ErrorCode::InvalidArguments);
    }
    let Ok(input) = ChatInput::validate(args.order, args.message.as_deref(), args.since) else {
        return refused(ErrorCode::InvalidArguments);
    };
    let reader = match resolve_reader(cli, false).await {
        Ok(reader) => reader,
        Err(code) => return refused(code),
    };
    let limits = ReadLimits::default();
    order_chat(&input, &reader.config, &reader.store, &limits, Utc::now()).await
}

/// A failure before any relay was contacted: nothing was published.
fn refused(code: ErrorCode) -> ChatOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::read(None),
    )
}
