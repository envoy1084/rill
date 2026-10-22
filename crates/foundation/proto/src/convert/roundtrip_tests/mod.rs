//! Round-trip tests through both the conversion layer and wire encoding,
//! so that what goes over the wire decodes to an equal `rill-types` value.

mod change;
mod descriptor;
mod event;
mod fixtures;
mod review;
mod tree;
mod workspace;
