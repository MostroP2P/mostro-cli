//! Human presentation. Renderers return text and perform no I/O.

pub mod order;
pub mod order_info;
pub mod table;

pub use order::render_list;
pub use order_info::render_info;
