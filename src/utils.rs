use chrono::Utc;
use get_if_addrs;
use std::collections::HashSet;
use std::net::IpAddr;

/// Get the current time.
///
/// Returns the current time in the form YYYY-mm-ddTHH:MM:SS+00:00
pub fn timestamp() -> String {
    let now = Utc::now();
    now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

/// Get all IP addresses
pub fn get_addresses() -> Vec<String> {
    let mut addresses = HashSet::new();

    for iface in get_if_addrs::get_if_addrs().unwrap() {
        match iface.ip() {
            IpAddr::V4(addr) => addresses.insert(addr.to_string()),
            IpAddr::V6(addr) => addresses.insert(format!("[{}]", addr.to_string())),
        };
    }

    let mut results = Vec::new();
    addresses.iter().for_each(|a| results.push(a.clone()));
    results.sort_unstable();

    results
}
