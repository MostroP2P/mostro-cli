pub mod chat;
pub mod counterparty;
pub mod dispute;
pub mod held;
pub mod info;
pub mod invoice;
pub mod list;
pub mod mine;
pub mod new;
pub mod rate;
pub mod step;
pub mod sync;
pub mod take;
pub mod watch;
mod write;

/// How a write command sends its request: the work to mine and whether to
/// check the node's protocol (off when the user chose the transport explicitly).
#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    pub pow: u8,
    pub check_protocol: bool,
}
