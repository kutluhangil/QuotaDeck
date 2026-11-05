//! Keep Awake: stops idle system sleep while it is on, the way `caffeinate -i` does. The
//! display may still sleep. A public power-management assertion, so every build has it; macOS
//! drops it by itself when the app quits.

use std::sync::Mutex;

/// The live assertion's id, `None` when off.
static ASSERTION: Mutex<Option<u32>> = Mutex::new(None);

#[cfg(target_os = "macos")]
mod pm {
    use std::ffi::c_void;

    use objc2_foundation::NSString;

    /// `kIOPMAssertionLevelOn`.
    const LEVEL_ON: u32 = 255;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            kind: *const c_void,
            level: u32,
            name: *const c_void,
            id: *mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    pub fn create() -> Option<u32> {
        // NSString is toll-free bridged to CFString.
        let kind = NSString::from_str("PreventUserIdleSystemSleep");
        let name = NSString::from_str("Quota Deck: Keep Awake");
        let mut id = 0u32;
        // SAFETY: both strings outlive the call; the id is written only on success.
        let status = unsafe {
            IOPMAssertionCreateWithName(
                objc2::rc::Retained::as_ptr(&kind).cast(),
                LEVEL_ON,
                objc2::rc::Retained::as_ptr(&name).cast(),
                &mut id,
            )
        };
        (status == 0).then_some(id)
    }

    pub fn release(id: u32) {
        // SAFETY: `id` came from a successful create and is released once.
        unsafe {
            IOPMAssertionRelease(id);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod pm {
    pub fn create() -> Option<u32> {
        None
    }
    pub fn release(_id: u32) {}
}

/// Turns Keep Awake on or off and returns the resulting state.
#[tauri::command]
pub fn set_keep_awake(enabled: bool) -> bool {
    let mut current = match ASSERTION.lock() {
        Ok(current) => current,
        Err(poisoned) => poisoned.into_inner(),
    };
    match (enabled, *current) {
        (true, None) => *current = pm::create(),
        (false, Some(id)) => {
            pm::release(id);
            *current = None;
        }
        _ => {}
    }
    current.is_some()
}

#[tauri::command]
pub fn keep_awake() -> bool {
    match ASSERTION.lock() {
        Ok(current) => current.is_some(),
        Err(poisoned) => poisoned.into_inner().is_some(),
    }
}
