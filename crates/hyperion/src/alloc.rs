use std::{
    alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc},
    ptr::null_mut,
    sync::OnceLock,
};

use flecs_ecs::sys::{
    ecs_os_api_calloc_t, ecs_os_api_free_t, ecs_os_api_malloc_t, ecs_os_api_realloc_t, ecs_size_t,
};

const ALIGNMENT: usize = 64;

// Global size tracker using papaya HashMap
static ALLOC_SIZES: OnceLock<papaya::HashMap<usize, usize>> = OnceLock::new();

fn init_size_map() {
    ALLOC_SIZES.get_or_init(papaya::HashMap::new);
}

fn get_size_map() -> &'static papaya::HashMap<usize, usize> {
    unsafe { ALLOC_SIZES.get().unwrap_unchecked() }
}

unsafe fn aligned_alloc(size: ecs_size_t, custom_alloc: fn(Layout) -> *mut u8) -> *mut core::ffi::c_void {
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let size = size as usize;

    // Allocate with our desired alignment
    let Ok(layout) = Layout::from_size_align(size, ALIGNMENT) else {
        return null_mut();
    };

    let ptr = unsafe { custom_alloc(layout) };

    if ptr.is_null() {
        return null_mut();
    }

    // Store the size in our global map
    get_size_map().pin().insert(ptr as usize, size);

    ptr.cast::<core::ffi::c_void>()
}

unsafe extern "C-unwind" fn aligned_malloc(size: ecs_size_t) -> *mut core::ffi::c_void {
    aligned_alloc(size, alloc)
}

unsafe extern "C-unwind" fn aligned_calloc(size: ecs_size_t) -> *mut core::ffi::c_void {
    aligned_alloc(size, alloc_zeroed)
}

unsafe extern "C-unwind" fn aligned_realloc(
    ptr: *mut core::ffi::c_void,
    new_size: ecs_size_t,
) -> *mut core::ffi::c_void {
    if ptr.is_null() {
        return unsafe { aligned_malloc(new_size) };
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let new_size = new_size as usize;

    let size_map = get_size_map().pin();

    // Get the old size from our map
    let old_size = size_map.get(&(ptr as usize)).copied().unwrap_or(0);

    // Create layout for reallocation
    let layout = unsafe { Layout::from_size_align_unchecked(old_size, ALIGNMENT) };

    let new_ptr = unsafe { realloc(ptr.cast::<u8>(), layout, new_size) };
    if new_ptr.is_null() {
        return null_mut();
    }

    // Update size in map
    size_map.remove(&(ptr as usize));
    size_map.insert(new_ptr as usize, new_size);

    new_ptr.cast::<core::ffi::c_void>()
}

unsafe extern "C-unwind" fn aligned_free(ptr: *mut core::ffi::c_void) {
    if !ptr.is_null() {
        let size_map = get_size_map().pin();
        // Get the size from our map
        if let Some(size) = size_map.remove(&(ptr as usize)) {
            // Deallocate the block
            let layout = unsafe { Layout::from_size_align_unchecked(*size, ALIGNMENT) };
            unsafe { dealloc(ptr.cast::<u8>(), layout) };
        }
    }
}

pub fn setup_custom_allocators() -> (
    ecs_os_api_malloc_t,
    ecs_os_api_calloc_t,
    ecs_os_api_realloc_t,
    ecs_os_api_free_t,
) {
    // Initialize the global size map if not already initialized
    init_size_map();

    (
        Some(aligned_malloc),
        Some(aligned_calloc),
        Some(aligned_realloc),
        Some(aligned_free),
    )
}
