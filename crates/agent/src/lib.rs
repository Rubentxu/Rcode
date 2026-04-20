//! Agent implementation
#![allow(
    clippy::collapsible_if,
    clippy::redundant_closure,
    unused_imports,
    unused_variables,
    unused_assignments
)]

pub mod executor;
pub mod delegation;
pub mod default_agent;
pub mod permissions;
pub mod subagent;
pub mod compaction;

pub use executor::{AgentExecutor, AgentExecutorBuilder, CancellationToken};
pub use delegation::DelegationManager;
pub use default_agent::DefaultAgent;
pub use permissions::{PermissionService, AutoPermissionService, InteractivePermissionService, RuleBasedPermissionService, is_sensitive_tool};
pub use subagent::{SubagentManager, SubagentId, SubagentInstance, SubagentStatus};
pub use compaction::{CompactionSignal, estimate_tokens, is_overflow, prune_tool_outputs, context_window_for_model};
