//! Windows fullscreen detection — filled in by C5. Stub returns false so
//! the cross-platform module compiles on Mac and the Windows target
//! cross-compile gate stays green.

pub fn is_fullscreen_now() -> bool {
    false
}
