use std::{
    alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc},
    ffi::c_void,
    mem, ptr,
};

const ALLOCATION_ALIGNMENT: usize = 16;
const HEADER_BYTES: usize = 16;

fn allocation_layout(payload_bytes: usize) -> Option<Layout> {
    let allocation_bytes = HEADER_BYTES.checked_add(payload_bytes)?;
    Layout::from_size_align(allocation_bytes, ALLOCATION_ALIGNMENT).ok()
}

unsafe fn write_payload_bytes(allocation: *mut u8, payload_bytes: usize) {
    let bytes = payload_bytes.to_ne_bytes();
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), allocation, mem::size_of::<usize>()) };
}

unsafe fn read_payload_bytes(allocation: *const u8) -> usize {
    let mut bytes = [0u8; mem::size_of::<usize>()];
    unsafe { ptr::copy_nonoverlapping(allocation, bytes.as_mut_ptr(), mem::size_of::<usize>()) };
    usize::from_ne_bytes(bytes)
}

#[unsafe(no_mangle)]
unsafe extern "C" fn gtl_tree_sitter_malloc(payload_bytes: usize) -> *mut c_void {
    if payload_bytes == 0 {
        return ptr::null_mut();
    }
    let Some(layout) = allocation_layout(payload_bytes) else {
        return ptr::null_mut();
    };
    let allocation = unsafe { alloc(layout) };
    if allocation.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        write_payload_bytes(allocation, payload_bytes);
        allocation.add(HEADER_BYTES).cast()
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn gtl_tree_sitter_calloc(count: usize, size: usize) -> *mut c_void {
    let Some(payload_bytes) = count.checked_mul(size) else {
        return ptr::null_mut();
    };
    if payload_bytes == 0 {
        return ptr::null_mut();
    }
    let Some(layout) = allocation_layout(payload_bytes) else {
        return ptr::null_mut();
    };
    let allocation = unsafe { alloc_zeroed(layout) };
    if allocation.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        write_payload_bytes(allocation, payload_bytes);
        allocation.add(HEADER_BYTES).cast()
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn gtl_tree_sitter_realloc(
    pointer: *mut c_void,
    payload_bytes: usize,
) -> *mut c_void {
    if pointer.is_null() {
        return unsafe { gtl_tree_sitter_malloc(payload_bytes) };
    }
    if payload_bytes == 0 {
        unsafe { gtl_tree_sitter_free(pointer) };
        return ptr::null_mut();
    }

    let allocation = unsafe { pointer.cast::<u8>().sub(HEADER_BYTES) };
    let old_payload_bytes = unsafe { read_payload_bytes(allocation) };
    let Some(old_layout) = allocation_layout(old_payload_bytes) else {
        return ptr::null_mut();
    };
    let Some(new_layout) = allocation_layout(payload_bytes) else {
        return ptr::null_mut();
    };
    let resized = unsafe { realloc(allocation, old_layout, new_layout.size()) };
    if resized.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        write_payload_bytes(resized, payload_bytes);
        resized.add(HEADER_BYTES).cast()
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn gtl_tree_sitter_free(pointer: *mut c_void) {
    if pointer.is_null() {
        return;
    }
    let allocation = unsafe { pointer.cast::<u8>().sub(HEADER_BYTES) };
    let payload_bytes = unsafe { read_payload_bytes(allocation) };
    if let Some(layout) = allocation_layout(payload_bytes) {
        unsafe { dealloc(allocation, layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_round_trips_through_resize_and_free() {
        unsafe {
            let allocation = gtl_tree_sitter_malloc(8).cast::<u8>();
            assert!(!allocation.is_null());
            for index in 0..8 {
                allocation
                    .add(index)
                    .write(u8::try_from(index).expect("fixture byte"));
            }

            let resized = gtl_tree_sitter_realloc(allocation.cast(), 32).cast::<u8>();
            assert!(!resized.is_null());
            for index in 0..8 {
                assert_eq!(
                    resized.add(index).read(),
                    u8::try_from(index).expect("fixture byte")
                );
            }
            gtl_tree_sitter_free(resized.cast());
        }
    }

    #[test]
    fn zeroed_allocation_and_overflow_follow_c_allocator_conventions() {
        unsafe {
            let allocation = gtl_tree_sitter_calloc(4, 8).cast::<u8>();
            assert!(!allocation.is_null());
            assert!((0..32).all(|index| allocation.add(index).read() == 0));
            gtl_tree_sitter_free(allocation.cast());

            assert!(gtl_tree_sitter_calloc(usize::MAX, 2).is_null());
            assert!(gtl_tree_sitter_malloc(0).is_null());
            gtl_tree_sitter_free(ptr::null_mut());
        }
    }
}
