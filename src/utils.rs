/// Utility functions.

use chrono::Utc;
use std::net::UdpSocket;

/// Get the current time.
///
/// Returns the current time in the form YYYY-mm-ddTHH:MM:SS+00:00
pub fn timestamp() -> String {
    let now = Utc::now();
    now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

/// Get the default local IP address.
///
/// From: https://stackoverflow.com/a/28950776
pub fn get_ip() -> String {
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    match socket.connect("10.255.255.255:1") {
        Ok(_) => match socket.local_addr() {
            Ok(addr) => addr.ip().to_string(),
            Err(_) => "127.0.0.1".to_string(),
        },
        Err(_) => "127.0.0.1".to_string(),
    }
}
