// Windows-specific platform code.
//
// Copy-on-write is handled by `reflink-copy` (ReFS-only on Windows).
// Junction creation via the `junction` crate is gated behind the `windows`
// feature flag.
