//! ACP (Agent Client Protocol) - JSON-RPC 2.0 over stdio for Zed IDE integration
#![allow(
    clippy::while_let_on_iterator,
    clippy::useless_conversion,
    unused_variables
)]

pub mod protocol;
pub mod server;
pub mod session;
pub mod events;

pub use protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError, JsonRpcErrorCode};
pub use server::AcpServer;
pub use session::ACPSessionManager;
