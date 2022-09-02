use chrono::Utc;

#[cfg(feature = "actix")]
use std::{collections::HashSet, net::IpAddr};

/// Get the current time.
///
/// Returns the current time in the form YYYY-mm-ddTHH:MM:SS+00:00
pub fn timestamp() -> String {
    let now = Utc::now();
    now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

/// Get all IP addresses
#[cfg(feature = "actix")]
pub fn get_addresses() -> Vec<String> {
    let mut addresses = HashSet::new();

    for iface in if_addrs::get_if_addrs().unwrap() {
        match iface.ip() {
            IpAddr::V4(addr) => addresses.insert(addr.to_string()),
            IpAddr::V6(addr) => addresses.insert(format!("[{}]", addr.to_string())),
        };
    }

    let mut results = Vec::with_capacity(addresses.len());
    results.extend(addresses.into_iter());
    results.sort_unstable();

    results
}
