//! The runner offline: the Claude backend against a local stand-in for
//! the Messages API, and the review and resolve loops with the fake
//! backend against an in-process dev server.

mod claude;
mod fleet;
