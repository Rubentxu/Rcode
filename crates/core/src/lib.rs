//! Core domain types and traits for rcode
//!
//! This crate contains the fundamental abstractions:
//! - `Agent` trait for agent implementations
//! - `Session`, `Message`, `Part` types for conversation state
//! - `Tool` trait for tool implementations
//! - `LlmProvider` trait for AI provider integrations
//! - `Skill` types for skill system

pub mod agent;
pub mod session;
pub mod message;
pub mod tool;
pub mod error;
pub mod provider;
pub mod config;
pub mod config_loader;
pub mod permission;
pub mod skill;
pub mod command;
pub mod agent_definition;
pub mod agent_loader;
pub mod dynamic_agent;
pub mod agent_registry;
pub mod subagent_runner;
pub mod auth;

// Explicit re-exports to avoid ambiguous glob re-exports
// agent::StopReason and provider::StopReason are different types, keep provider's as canonical
pub use agent::StopReason as AgentStopReason;
pub use agent::AgentInfo;
pub use agent::AgentResult;
pub use agent::AgentContext;
pub use agent::Agent;
// agent_definition and config both export AgentMode and AgentPermissionConfig (different types)
// Keep config's versions as canonical, rename agent_definition's
pub use agent_definition::AgentMode as AgentDefinitionMode;
pub use agent_definition::AgentPermissionConfig as AgentDefinitionPermissionConfig;
pub use agent_definition::AgentDefinition;
pub use agent_definition::TaskPermission;
pub use session::*;
pub use message::*;
pub use tool::*;
pub use error::*;
pub use provider::*;
pub use config::*;
pub use config_loader::*;
pub use permission::*;
pub use skill::*;
pub use command::*;
pub use agent_loader::AgentLoader;
pub use dynamic_agent::DynamicAgent;
pub use agent_registry::AgentRegistry;
pub use subagent_runner::SubagentRunner;
pub use auth::*;
