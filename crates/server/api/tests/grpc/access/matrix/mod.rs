//! The permission matrix: every RPC, classified by what it needs, called
//! as every kind of caller. A new RPC fails the coverage test until it is
//! classified here. A path-limited reader's answers are searched for
//! anything from the directory it cannot read.

mod callers;
mod classes;
mod coverage;
mod hidden;
mod leaks;
mod probes;
mod roles;
