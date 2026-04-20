//! Core domain types and traits for rcode
//!
//! This crate contains the fundamental abstractions:
//! - `Agent` trait for agent implementations
//! - `Session`, `Message`, `Part` types for conversation state
//! - `Tool` trait for tool implementations
//! - `LlmProvider` trait for AI provider integrations
//! - `Skill` types for skill system

pub mod agent;
pub mod agent_definition;
pub mod agent_loader;
pub mod agent_registry;
pub mod auth;
pub mod command;
pub mod config;
pub mod config_loader;
pub mod dynamic_agent;
pub mod error;
pub mod message;
pub mod permission;
pub mod project;
pub mod provider;
pub mod session;
pub mod skill;
pub mod subagent_runner;
pub mod tool;

// Explicit re-exports to avoid ambiguous glob re-exports
// agent::StopReason and provider::StopReason are different types, keep provider's as canonical
pub use agent::Agent;
pub use agent::AgentContext;
pub use agent::AgentInfo;
pub use agent::AgentResult;
pub use agent::StopReason as AgentStopReason;
// agent_definition and config both export AgentMode and AgentPermissionConfig (different types)
// Keep config's versions as canonical, rename agent_definition's
pub use agent_definition::AgentDefinition;
pub use agent_definition::AgentMode as AgentDefinitionMode;
pub use agent_definition::AgentPermissionConfig as AgentDefinitionPermissionConfig;
pub use agent_definition::TaskPermission;
pub use agent_loader::AgentLoader;
pub use agent_registry::AgentRegistry;
pub use auth::*;
pub use command::*;
#[allow(ambiguous_glob_reexports)]
pub use config::*;
pub use config_loader::*;
pub use dynamic_agent::DynamicAgent;
pub use error::*;
pub use message::*;
pub use permission::*;
pub use project::*;
pub use provider::*;
pub use session::*;
pub use skill::*;
pub use subagent_runner::{SubagentRunner, SubagentResult};
pub use tool::*;
