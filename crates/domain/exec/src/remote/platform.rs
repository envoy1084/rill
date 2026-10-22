//! Platforms: the properties an action needs from a worker. Actions and
//! workers with the same properties share a queue.

use rill_reapi_proto::reapi::Platform;

/// A short stable key for `platform`: properties sorted by name and value.
pub fn platform_key(platform: Option<&Platform>) -> String {
    let mut props: Vec<(&str, &str)> = platform
        .map(|p| {
            p.properties
                .iter()
                .map(|p| (p.name.as_str(), p.value.as_str()))
                .collect()
        })
        .unwrap_or_default();
    props.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for (name, value) in props {
        hasher.update(name.as_bytes());
        hasher.update(b"=");
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex()[..16].to_string()
}

/// The job queue holding actions for the platform `key`.
pub fn queue_name(key: &str) -> String {
    format!("exec/{key}")
}

#[cfg(test)]
mod tests {
    use rill_reapi_proto::reapi::platform::Property;

    use super::*;

    fn platform(props: &[(&str, &str)]) -> Platform {
        Platform {
            properties: props
                .iter()
                .map(|(n, v)| Property {
                    name: n.to_string(),
                    value: v.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn property_order_does_not_change_the_queue() {
        let a = platform(&[("OSFamily", "linux"), ("arch", "x86_64")]);
        let b = platform(&[("arch", "x86_64"), ("OSFamily", "linux")]);
        assert_eq!(platform_key(Some(&a)), platform_key(Some(&b)));
        assert_ne!(
            platform_key(Some(&a)),
            platform_key(Some(&platform(&[("OSFamily", "linux")])))
        );
        assert_eq!(platform_key(None), platform_key(Some(&platform(&[]))));
    }
}
