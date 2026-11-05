//! Human presentation. Renderers return text and perform no I/O.

pub mod order;
pub mod table;

pub use order::render_list;
