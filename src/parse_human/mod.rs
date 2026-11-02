//! Human presentation. Renderers return text and perform no I/O.

pub mod invoice;
pub mod message;
pub mod order;
pub mod order_info;
pub mod order_new;
pub mod order_take;
pub mod step;
pub mod table;

pub use invoice::{render_bond_invoice, render_invoice_preview, render_order_invoice};
pub use message::render_messages;
pub use order::render_list;
pub use order_info::render_info;
pub use order_new::{render_new, render_preview};
pub use order_take::{render_take_preview, render_taken};
pub use step::{render_step, render_step_preview};
