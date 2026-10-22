//! Verify plans end to end: compiled to actions, queued on the scheduler
//! and run by a local worker with real processes.

#![cfg(unix)]

mod fixture;
mod graph;
mod history;
mod running;
mod secrets;
mod selection;
