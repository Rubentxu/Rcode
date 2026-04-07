//! Built-in tools for RCode
#![allow(
    clippy::collapsible_if,
    clippy::redundant_closure,
    clippy::manual_flatten,
    clippy::let_and_return,
    clippy::bind_instead_of_map,
    clippy::new_without_default,
    clippy::needless_range_loop,
    clippy::io_other_error,
    clippy::bool_comparison,
    clippy::unnecessary_unwrap,
    clippy::useless_format,
    clippy::default_constructed_unit_structs,
    unused_imports,
    unused_variables
)]

pub mod bash;
pub mod batch;
pub mod question;
pub mod read;
pub mod write;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod task;
pub mod registry;
pub mod validator;
pub mod mock;
pub mod skill_discovery;
pub mod skill_registry;
pub mod skill_tool;
pub mod mcp_tool;
pub mod plan;
pub mod command_discovery;
pub mod command_registry;
pub mod slash_command_tool;
pub mod todowrite;
pub mod plan_exit;
pub mod delegate;
pub mod webfetch;
pub mod websearch;
pub mod codesearch;
pub mod session_navigation;
pub mod applypatch;

pub use registry::ToolRegistryService;
pub use validator::ToolValidator;
pub use skill_discovery::SkillDiscovery;
pub use skill_registry::SkillRegistry;
pub use skill_tool::SkillTool;
pub use mcp_tool::McpToolAdapter;
pub use plan::PlanTool;
pub use command_discovery::CommandDiscovery;
pub use command_registry::CommandRegistry;
pub use slash_command_tool::SlashCommandTool;
pub use delegate::{DelegateTool, DelegationReadTool, DelegationRecord};
