//! Everything that talks to the League Client (LCU) or the Live Client Data API.
//!
//! The LCU is unofficial and changes between patches, so all knowledge of its
//! paths, auth and payload shapes is kept in this module.

pub mod api;
pub mod client;
pub mod discovery;
pub mod ws;

pub use client::{LcuClient, LiveClient, TlsMode};
