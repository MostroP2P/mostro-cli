use anyhow::Result;
use mostro_core::prelude::Action;
use nostr_sdk::prelude::PublicKey;
use std::str::FromStr;

use super::add_bond_invoice::execute_add_bond_invoice;
use super::add_invoice::execute_add_invoice;
use super::adm_send_dm::execute_adm_send_dm;
use super::commands::Commands;
use super::context::Context;
use super::conversation_key::execute_conversation_key;
use super::dispute_chat::execute_dispute_chat;
use super::dm_to_user::execute_dm_to_user;
use super::get_dm::execute_get_dm;
use super::get_dm_user::execute_get_dm_user;
use super::last_trade_index::{execute_last_trade_index, execute_last_trade_index_private_key};
use super::list_disputes::execute_list_disputes;
use super::maintenance::{
    execute_cancel_pending, execute_maintenance_status, execute_set_maintenance,
};
use super::rate_user::execute_rate_user;
use super::restore::execute_restore;
use super::send_admin_dm_attach::execute_send_admin_dm_attach;
use super::send_dm::execute_send_dm;
use super::take_dispute::{
    execute_admin_add_solver, execute_admin_cancel_dispute, execute_admin_settle_dispute,
    execute_take_dispute,
};
use super::take_order::execute_take_order;
use super::user_info::execute_user_info;
use crate::util::run_simple_order_msg;

impl Commands {
    /// Run a command that talks to `mostrod`'s admin gRPC directly. `None`
    /// when the command is a Nostr one and needs a [`Context`].
    pub async fn run_rpc(&self) -> Option<Result<()>> {
        match self {
            Commands::AdmSetMaintenance { enabled, reason } => {
                Some(execute_set_maintenance(*enabled, reason.clone()).await)
            }
            Commands::AdmMaintenanceStatus {} => Some(execute_maintenance_status().await),
            Commands::AdmCancelPending { order_id } => {
                Some(execute_cancel_pending(*order_id).await)
            }
            _ => None,
        }
    }

    pub async fn run(&self, ctx: &Context) -> Result<()> {
        match self {
            // Simple order message commands
            Commands::FiatSent { order_id }
            | Commands::Release { order_id }
            | Commands::Dispute { order_id }
            | Commands::Cancel { order_id } => {
                run_simple_order_msg(self.clone(), Some(*order_id), ctx).await
            }
            // Last trade index commands
            Commands::GetLastTradeIndex {} => {
                execute_last_trade_index(&ctx.identity_keys, ctx.mostro_pubkey, ctx).await
            }
            Commands::GetLastTradePrivkey {} => execute_last_trade_index_private_key(ctx).await,
            Commands::GetUserInfo {} => execute_user_info(ctx).await,
            // DM commands with pubkey parsing
            Commands::SendDm {
                pubkey,
                order_id,
                message,
            } => {
                let msg = message.join(" ");
                execute_send_dm(PublicKey::from_str(pubkey)?, ctx, order_id, &msg).await
            }
            Commands::DmToUser {
                pubkey,
                order_id,
                message,
            } => {
                let msg = message.join(" ");
                execute_dm_to_user(
                    PublicKey::from_str(pubkey)?,
                    &ctx.client,
                    order_id,
                    &msg,
                    &ctx.pool,
                )
                .await
            }
            Commands::AdmSendDm { pubkey, message } => {
                let msg = message.join(" ");
                execute_adm_send_dm(PublicKey::from_str(pubkey)?, ctx, &msg).await
            }
            Commands::SendAdminDmAttach {
                pubkey,
                order_id,
                file,
            } => {
                execute_send_admin_dm_attach(PublicKey::from_str(pubkey)?, ctx, order_id, file)
                    .await
            }
            Commands::ConversationKey { pubkey } => {
                execute_conversation_key(&ctx.trade_keys, PublicKey::from_str(pubkey)?).await
            }

            // Order management commands
            Commands::Order { .. } => unreachable!("public listing runs without a Context"),
            Commands::Schema => unreachable!("discovery runs without a Context"),
            Commands::TakeSell {
                order_id,
                invoice,
                amount,
            } => execute_take_order(order_id, Action::TakeSell, invoice, *amount, ctx).await,
            Commands::TakeBuy { order_id, amount } => {
                execute_take_order(order_id, Action::TakeBuy, &None, *amount, ctx).await
            }
            Commands::AddInvoice { order_id, invoice } => {
                execute_add_invoice(order_id, invoice, ctx).await
            }
            Commands::AddBondInvoice { order_id, invoice } => {
                execute_add_bond_invoice(order_id, invoice, ctx).await
            }
            Commands::Rate { order_id, rating } => execute_rate_user(order_id, rating, ctx).await,

            // DM retrieval commands
            Commands::GetDm { since, from_user } => {
                execute_get_dm(since, false, from_user, ctx).await
            }
            Commands::GetDmUser {
                pubkey,
                order_id,
                since,
            } => execute_get_dm_user(PublicKey::from_str(pubkey)?, *order_id, since, ctx).await,
            Commands::DisputeChat {
                order_id,
                since,
                message,
            } => execute_dispute_chat(*order_id, since, message.as_deref(), ctx).await,
            Commands::GetAdminDm { since, from_user } => {
                execute_get_dm(since, true, from_user, ctx).await
            }

            // Admin commands
            Commands::ListDisputes {} => execute_list_disputes(ctx).await,
            Commands::AdmAddSolver { npubkey } => execute_admin_add_solver(npubkey, ctx).await,
            Commands::AdmSettle {
                order_id,
                slash_seller,
                slash_buyer,
            } => execute_admin_settle_dispute(order_id, *slash_seller, *slash_buyer, ctx).await,
            Commands::AdmCancel {
                order_id,
                slash_seller,
                slash_buyer,
            } => execute_admin_cancel_dispute(order_id, *slash_seller, *slash_buyer, ctx).await,
            Commands::AdmTakeDispute { dispute_id } => execute_take_dispute(dispute_id, ctx).await,
            Commands::AdmSetMaintenance { .. }
            | Commands::AdmMaintenanceStatus {}
            | Commands::AdmCancelPending { .. } => {
                unreachable!("handled by run_rpc before a Context is built")
            }

            // Simple commands
            Commands::Restore {} => {
                execute_restore(&ctx.identity_keys, ctx.mostro_pubkey, ctx).await
            }
        }
    }
}
