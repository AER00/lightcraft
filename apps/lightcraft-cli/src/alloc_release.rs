//! Give memory the system allocator keeps after frees back to the system
//! ([`lightcraft_engine::memory::set_release_hook`]).
//!
//! macOS's allocator keeps freed large blocks mapped as "reusable": they no longer count in the
//! process's memory footprint but stay in its resident size until the system runs short. After a
//! raw decode (hundreds of MB of temporaries) that made the resident size climb towards the sum of
//! everything ever decoded. `malloc_zone_pressure_relief` (libSystem) returns those pages now.
//! Elsewhere (glibc, Windows) blocks this large are unmapped when freed: nothing to do.

/// Install the hook for this platform.
pub fn install() {
    #[cfg(target_os = "macos")]
    lightcraft_engine::memory::set_release_hook(macos::release);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos {
    unsafe extern "C" {
        /// libSystem malloc: release free memory of `zone` (all zones when null) back to the
        /// system, up to `goal` bytes (0 = as much as possible). Returns the bytes released.
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }

    pub fn release() {
        // SAFETY: a documented libSystem function taking a null zone (= every zone) and a size;
        // it only returns free pages to the system and touches no memory we own.
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
}
