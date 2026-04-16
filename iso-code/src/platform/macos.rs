// macOS-specific platform code.
//
// Copy-on-write is handled by `reflink-copy` (clonefile(2) on APFS).
// Network-filesystem detection reads `statfs::f_fstypename`.
