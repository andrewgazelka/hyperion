use std::{
    alloc::{Layout, alloc, dealloc, realloc},
    ptr::null_mut,
};

use flecs_ecs::sys::{
    ecs_os_api_calloc_t, ecs_os_api_free_t, ecs_os_api_malloc_t, ecs_os_api_realloc_t, ecs_size_t,
};

const ALIGNMENT: usize = 64;

// Store size in a prefix, perfectly aligned
#[repr(C, align(64))] // Ensure the header itself is aligned
struct AllocHeader {
    size: usize,
}

unsafe extern "C-unwind" fn aligned_malloc(size: ecs_size_t) -> *mut core::ffi::c_void {
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let total_size = size as usize + size_of::<AllocHeader>();

    // Allocate with our desired alignment
    let Ok(layout) = Layout::from_size_align(total_size, ALIGNMENT) else {
        return null_mut();
    };

    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return null_mut();
    }

    // Write the header
    #[allow(clippy::cast_ptr_alignment)]
    let header = ptr.cast::<AllocHeader>();

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    unsafe {
        (*header).size = size as usize;
    };

    // Return pointer after the header
    unsafe {
        ptr.add(size_of::<AllocHeader>())
            .cast::<core::ffi::c_void>()
    }
}

unsafe extern "C-unwind" fn aligned_calloc(size: ecs_size_t) -> *mut core::ffi::c_void {
    let ptr = unsafe { aligned_malloc(size) };
    if !ptr.is_null() {
        // Zero only the user data portion, header already contains size
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        unsafe {
            std::ptr::write_bytes(ptr, 0, size as usize);
        };
    }
    ptr
}

unsafe extern "C-unwind" fn aligned_realloc(
    ptr: *mut core::ffi::c_void,
    new_size: ecs_size_t,
) -> *mut core::ffi::c_void {
    if ptr.is_null() {
        return unsafe { aligned_malloc(new_size) };
    }

    // Get the header pointer from the user pointer
    #[allow(clippy::cast_ptr_alignment)]
    let header_ptr = unsafe {
        ptr.cast::<u8>()
            .sub(size_of::<AllocHeader>())
            .cast::<AllocHeader>()
    };
    let old_size = unsafe { (*header_ptr).size };

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let total_new_size = new_size as usize + size_of::<AllocHeader>();

    // Reallocate with the total size
    let layout = unsafe {
        Layout::from_size_align_unchecked(old_size + size_of::<AllocHeader>(), ALIGNMENT)
    };

    let new_ptr = unsafe { realloc(header_ptr.cast::<u8>(), layout, total_new_size) };

    if new_ptr.is_null() {
        return null_mut();
    }

    // Update size in header
    #[allow(clippy::cast_ptr_alignment)]
    let new_header = new_ptr.cast::<AllocHeader>();

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    unsafe {
        (*new_header).size = new_size as usize;
    };

    // Return pointer after header
    unsafe {
        new_ptr
            .add(size_of::<AllocHeader>())
            .cast::<core::ffi::c_void>()
    }
}

unsafe extern "C-unwind" fn aligned_free(ptr: *mut core::ffi::c_void) {
    if !ptr.is_null() {
        // Get the header pointer
        let header_ptr = unsafe { ptr.cast::<u8>().sub(size_of::<AllocHeader>()) };

        #[allow(clippy::cast_ptr_alignment)]
        let header = header_ptr.cast::<AllocHeader>();
        let total_size = unsafe { (*header).size + size_of::<AllocHeader>() };

        // Deallocate the entire block
        let layout = unsafe { Layout::from_size_align_unchecked(total_size, ALIGNMENT) };
        unsafe { dealloc(header_ptr, layout) };
    }
}

// Setup function remains the same
pub fn setup_custom_allocators() -> (
    ecs_os_api_malloc_t,
    ecs_os_api_calloc_t,
    ecs_os_api_realloc_t,
    ecs_os_api_free_t,
) {
    (
        Some(aligned_malloc),
        Some(aligned_calloc),
        Some(aligned_realloc),
        Some(aligned_free),
    )
}
