pub(crate) mod acp_content;
pub(crate) mod acp_state;
pub(crate) mod agent;
pub(crate) mod dispatch;
pub(crate) mod display;
pub(crate) mod event_mapper;
pub(crate) mod handler;
pub(crate) mod slash_command;

#[cfg(test)]
mod agent_test;

pub use agent::serve;
