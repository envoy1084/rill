//! Protobuf timestamps.

use rill_types::Timestamp;

/// `millis` since the epoch as a protobuf timestamp.
pub(crate) fn proto_timestamp(millis: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: millis.div_euclid(1000),
        nanos: (millis.rem_euclid(1000) * 1_000_000) as i32,
    }
}

/// The current time as a protobuf timestamp.
pub(crate) fn proto_now() -> prost_types::Timestamp {
    proto_timestamp(Timestamp::now().millis())
}
