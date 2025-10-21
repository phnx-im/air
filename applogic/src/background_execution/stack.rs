// src/stack.rs
pub struct StackInfo {
    pub low: usize,
    pub high: usize,
}

#[inline]
fn stack_pointer() -> usize {
    let x = 0u8;
    (&x as *const u8) as usize
}

#[cfg(target_os = "ios")]
fn os_stack_bounds() -> (usize, usize) {
    use libc::{pthread_get_stackaddr_np, pthread_get_stacksize_np, pthread_self};
    unsafe {
        let t = pthread_self();
        let high = pthread_get_stackaddr_np(t) as usize;
        let size = pthread_get_stacksize_np(t) as usize;
        let low = high - size;
        (low, high)
    }
}

#[cfg(target_os = "android")]
fn os_stack_bounds() -> (usize, usize) {
    use libc::{
        c_void, pthread_attr_destroy, pthread_attr_getstack, pthread_attr_t, pthread_getattr_np,
        pthread_self,
    };
    unsafe {
        let mut attr: pthread_attr_t = core::mem::zeroed();
        if pthread_getattr_np(pthread_self(), &mut attr) != 0 {
            return (0, 0);
        }
        let mut base: *mut c_void = core::ptr::null_mut();
        let mut size: usize = 0;
        let _ = pthread_attr_getstack(&attr, &mut base, &mut size);
        let _ = pthread_attr_destroy(&mut attr);
        let low = base as usize;
        let high = low + size;
        (low, high)
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn os_stack_bounds() -> (usize, usize) {
    (0, 0)
}

pub(crate) fn info() -> StackInfo {
    let (low, high) = os_stack_bounds();
    StackInfo { low, high }
}

pub(crate) fn size() -> usize {
    let s = info();
    s.high - s.low
}

/// Approximate remaining bytes until the guard page.
/// Stack grows downward on both iOS and Android.
pub(crate) fn remaining() -> usize {
    let sp = stack_pointer();
    let s = info();
    sp.saturating_sub(s.low)
}
