//! Three servers behind a load balancer serve the end-to-end scenario, and
//! killing any of them, even between doing a write and answering it, loses
//! nothing: clients retry with the same operation id and get one outcome.

mod balancer;
mod replica;
mod retrying;
mod survival;
