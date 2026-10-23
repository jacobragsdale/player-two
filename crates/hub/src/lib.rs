//! The hub: one server the team shares, holding its chats in one SQLite file.
//!
//! The library is the whole hub; `main.rs` reads its settings and starts it, and the tests start
//! it in-process.

pub mod app;
pub mod clock;
pub mod db;
pub mod rules;
pub mod store;
pub mod stream;
