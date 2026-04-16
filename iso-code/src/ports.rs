//! Port lease allocation.
//!
//! Assignment is deterministic: the first four bytes of
//! `SHA-256(repo_id:branch)` are interpreted as a big-endian `u32`, reduced
//! modulo the configured range size, and offset by `port_range_start`.
//! Collisions fall back to sequential probing.

use sha2::{Digest, Sha256};

use crate::error::WorktreeError;
use crate::types::PortLease;

/// Compute the preferred port for a branch within a port range.
///
/// Formula: port_range_start + (sha256(format!("{repo_id}:{branch}"))[0..4] as u32 % range_size)
pub fn compute_preferred_port(
    repo_id: &str,
    branch: &str,
    port_range_start: u16,
    port_range_end: u16,
) -> u16 {
    let range_size = u32::from(port_range_end - port_range_start);
    let mut hasher = Sha256::new();
    hasher.update(format!("{repo_id}:{branch}").as_bytes());
    let hash = hasher.finalize();
    // Take first 4 bytes as u32 big-endian
    let val = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
    port_range_start + (val % range_size) as u16
}

/// Check if a port lease is expired.
pub fn is_lease_expired(lease: &PortLease, now: chrono::DateTime<chrono::Utc>) -> bool {
    lease.expires_at <= now
}

/// Allocate a port for a branch by probing from preferred port, wrapping around.
///
/// Returns the allocated port number.
pub fn allocate_port(
    repo_id: &str,
    branch: &str,
    _session_uuid: &str,
    port_range_start: u16,
    port_range_end: u16,
    existing_leases: &std::collections::HashMap<String, PortLease>,
) -> Result<u16, WorktreeError> {
    let range_size = (port_range_end - port_range_start) as u32;
    if range_size == 0 {
        return Err(WorktreeError::RateLimitExceeded {
            current: 0,
            max: 0,
        });
    }

    let now = chrono::Utc::now();
    let preferred = compute_preferred_port(repo_id, branch, port_range_start, port_range_end);
    let preferred_offset = preferred - port_range_start;

    // Collect taken ports (non-expired)
    let taken: std::collections::HashSet<u16> = existing_leases
        .values()
        .filter(|l| !is_lease_expired(l, now))
        .map(|l| l.port)
        .collect();

    // Linear probe from preferred, wrapping around
    for i in 0..range_size {
        let offset = (u32::from(preferred_offset) + i) % range_size;
        let port = port_range_start + offset as u16;
        if !taken.contains(&port) {
            return Ok(port);
        }
    }

    Err(WorktreeError::RateLimitExceeded {
        current: range_size as usize,
        max: range_size as usize,
    })
}

/// Build a PortLease with 8-hour TTL.
pub fn make_lease(
    port: u16,
    branch: &str,
    session_uuid: &str,
    pid: u32,
) -> PortLease {
    let now = chrono::Utc::now();
    PortLease {
        port,
        branch: branch.to_string(),
        session_uuid: session_uuid.to_string(),
        pid,
        created_at: now,
        expires_at: now + chrono::Duration::hours(8),
        status: "active".to_string(),
    }
}

/// Renew a lease — extend expires_at by another 8 hours from now.
pub fn renew_lease(lease: &mut PortLease) {
    lease.expires_at = chrono::Utc::now() + chrono::Duration::hours(8);
}

/// Sweep expired leases from the map, returning how many were removed.
pub fn sweep_expired_leases(
    leases: &mut std::collections::HashMap<String, PortLease>,
    now: chrono::DateTime<chrono::Utc>,
) -> usize {
    let expired_keys: Vec<String> = leases
        .iter()
        .filter(|(_, l)| is_lease_expired(l, now))
        .map(|(k, _)| k.clone())
        .collect();
    let count = expired_keys.len();
    for k in expired_keys {
        leases.remove(&k);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const REPO_ID: &str = "test-repo-abc123";
    const START: u16 = 3100;
    const END: u16 = 5100;

    #[test]
    fn preferred_port_is_deterministic() {
        let p1 = compute_preferred_port(REPO_ID, "main", START, END);
        let p2 = compute_preferred_port(REPO_ID, "main", START, END);
        assert_eq!(p1, p2);
        assert!((START..END).contains(&p1));
    }

    #[test]
    fn preferred_port_is_in_range() {
        let port = compute_preferred_port(REPO_ID, "feature/test", START, END);
        assert!((START..END).contains(&port));
    }

    #[test]
    fn allocate_no_collision() {
        let leases = HashMap::new();
        let port = allocate_port(REPO_ID, "main", "uuid-1", START, END, &leases).unwrap();
        assert!((START..END).contains(&port));
    }

    #[test]
    fn allocate_probes_on_collision() {
        let preferred = compute_preferred_port(REPO_ID, "branch-a", START, END);

        // Occupy the preferred port
        let mut leases = HashMap::new();
        let lease = make_lease(preferred, "other", "uuid-other", 1234);
        leases.insert("other".to_string(), lease);

        let port = allocate_port(REPO_ID, "branch-a", "uuid-a", START, END, &leases).unwrap();
        assert_ne!(port, preferred); // Must have probed to a different port
        assert!((START..END).contains(&port));
    }

    #[test]
    fn allocate_full_range_returns_error() {
        // Small range: 3100-3102 (2 ports)
        let start: u16 = 3100;
        let end: u16 = 3102;

        let mut leases = HashMap::new();
        // Fill both ports
        let now = chrono::Utc::now();
        let expires = now + chrono::Duration::hours(8);
        for port in start..end {
            leases.insert(
                port.to_string(),
                PortLease {
                    port,
                    branch: format!("branch-{port}"),
                    session_uuid: format!("uuid-{port}"),
                    pid: 1,
                    created_at: now,
                    expires_at: expires,
                    status: "active".to_string(),
                },
            );
        }

        let result = allocate_port(REPO_ID, "new-branch", "uuid-new", start, end, &leases);
        assert!(result.is_err());
    }

    #[test]
    fn expired_lease_frees_port() {
        let preferred = compute_preferred_port(REPO_ID, "branch-b", START, END);

        // Expired lease on preferred port
        let mut leases = HashMap::new();
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        leases.insert(
            "expired".to_string(),
            PortLease {
                port: preferred,
                branch: "other".to_string(),
                session_uuid: "uuid-exp".to_string(),
                pid: 9999,
                created_at: past - chrono::Duration::hours(8),
                expires_at: past,
                status: "active".to_string(),
            },
        );

        // Should get the preferred port since the lease is expired
        let port = allocate_port(REPO_ID, "branch-b", "uuid-b", START, END, &leases).unwrap();
        assert_eq!(port, preferred);
    }

    #[test]
    fn twenty_branches_get_unique_ports() {
        // Use a small range to force probing
        let start: u16 = 3100;
        let end: u16 = 3120; // Only 20 ports

        let mut leases = HashMap::new();
        let mut allocated = std::collections::HashSet::new();

        for i in 0..20u32 {
            let branch = format!("branch-{i}");
            let session = format!("uuid-{i}");
            let port = allocate_port(REPO_ID, &branch, &session, start, end, &leases).unwrap();
            assert!(allocated.insert(port), "port {port} was allocated twice");
            leases.insert(branch.clone(), make_lease(port, &branch, &session, 1234));
        }

        assert_eq!(allocated.len(), 20);
    }

    #[test]
    fn sweep_removes_expired_leases() {
        let mut leases = HashMap::new();
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let future = chrono::Utc::now() + chrono::Duration::hours(7);

        leases.insert(
            "expired".to_string(),
            PortLease {
                port: 3100,
                branch: "old".to_string(),
                session_uuid: "u1".to_string(),
                pid: 1,
                created_at: past - chrono::Duration::hours(8),
                expires_at: past,
                status: "active".to_string(),
            },
        );
        leases.insert(
            "active".to_string(),
            PortLease {
                port: 3101,
                branch: "new".to_string(),
                session_uuid: "u2".to_string(),
                pid: 2,
                created_at: chrono::Utc::now() - chrono::Duration::hours(1),
                expires_at: future,
                status: "active".to_string(),
            },
        );

        let removed = sweep_expired_leases(&mut leases, chrono::Utc::now());
        assert_eq!(removed, 1);
        assert!(!leases.contains_key("expired"));
        assert!(leases.contains_key("active"));
    }

    #[test]
    fn renew_extends_expiry() {
        let mut lease = make_lease(3100, "main", "uuid-1", 1234);
        let original_expiry = lease.expires_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        renew_lease(&mut lease);
        assert!(lease.expires_at > original_expiry);
    }
}
