//! Human presentation. Renderers return text and perform no I/O.

pub mod held;
pub mod identity;
pub mod invoice;
pub mod message;
pub mod mine;
pub mod order;
pub mod order_info;
pub mod order_new;
pub mod order_take;
pub mod step;
pub mod sync;
pub mod table;

pub use held::{render_dispute, render_dispute_preview, render_rate, render_rate_preview};
pub use identity::{render_identity_init, render_identity_show};
pub use invoice::{render_bond_invoice, render_invoice_preview, render_order_invoice};
pub use message::render_messages;
pub use mine::render_mine;
pub use order::render_list;
pub use order_info::render_info;
pub use order_new::{render_new, render_preview};
pub use order_take::{render_take_preview, render_taken};
pub use step::{render_step, render_step_preview};
pub use sync::render_sync;
