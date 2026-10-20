//! Recorded handoffs to existing local agent processes. No PTY or spawn API.
pub mod adapters;
pub mod identity;
pub mod journal;
pub mod observations;
pub mod presentation;
pub mod state;
pub mod worker;

#[cfg(test)]
mod tests;
