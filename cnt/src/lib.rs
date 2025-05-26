use crate::consts::BUF_SIZE;

mod consts;

#[unsafe(no_mangle)]
static mut _CNT_BUFFER: [u32; BUF_SIZE] = [0; BUF_SIZE];

// TODO: Use atomics? and if not available - critical section
#[inline(always)]
pub unsafe fn increment_u32(counter_idx: usize) {
    let buffer = unsafe {
        core::slice::from_raw_parts_mut(&raw mut _CNT_BUFFER as *mut _ as *mut u32, BUF_SIZE)
    };
    buffer[counter_idx] = buffer[counter_idx].saturating_add(1);
}

#[inline(always)]
pub unsafe fn increment_u64(counter_idx: usize) {
    let buffer = unsafe {
        core::slice::from_raw_parts_mut(&raw mut _CNT_BUFFER as *mut _ as *mut u32, BUF_SIZE)
    };
    let lo = buffer[counter_idx];
    let hi = buffer[counter_idx + 1];
    if hi == u32::MAX {
        buffer[counter_idx] = lo.saturating_add(1);
    } else {
        let (lo, overflowed) = lo.overflowing_add(1);
        buffer[counter_idx] = lo;
        if overflowed {
            buffer[counter_idx + 1] = hi.saturating_add(1);
        }
    }
}
