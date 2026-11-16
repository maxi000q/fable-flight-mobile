//! Links Android's liblog for the crate's hand-written logcat binding
//! (lib.rs `android_logging`). A whole crate rather than a build script, so
//! the link directive cannot race with parallel builds.
//!
//! Nothing here is called from Rust; the extern block in lib.rs is the user.

#[link(name = "log")]
extern "C" {}
