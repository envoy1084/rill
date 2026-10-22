//! Verify plans end to end: compiled to actions, queued on the scheduler
//! and run by a local worker with real processes.

#![cfg(unix)]

mod fixture;
mod graph;
mod hermetic;
mod history;
mod quarantine;
mod regenerate;
mod running;
mod runs;
mod secrets;
mod selection;
