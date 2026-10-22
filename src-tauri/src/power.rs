//! Keeps the computer awake while downloads are running.
//!
//! Only *idle* system sleep is prevented; the display may still turn off and
//! the screen may lock. Closing the lid or choosing "Sleep" still works.
//!
//! * macOS: IOKit power assertion `PreventUserIdleSystemSleep`
//!   (what `caffeinate -i` does).
//! * Windows: `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`.
//!   The state belongs to the calling thread, so a dedicated thread owns it.
//! * Other platforms: no-op.

use std::sync::mpsc;

pub struct SleepGuard {
    tx: mpsc::Sender<bool>,
}

impl SleepGuard {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<bool>();
        std::thread::Builder::new()
            .name("odm-power".into())
            .spawn(move || {
                #[allow(clippy::default_constructed_unit_structs)]
                let mut held = platform::Hold::default();
                for wanted in rx {
                    if wanted {
                        held.acquire();
                    } else {
                        held.release();
                    }
                }
                held.release();
            })
            .expect("cannot start power management thread");
        SleepGuard { tx }
    }

    pub fn set(&self, keep_awake: bool) {
        let _ = self.tx.send(keep_awake);
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{c_char, c_void, CString};

    type CFStringRef = *const c_void;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_IOPM_ASSERTION_LEVEL_ON: u32 = 255;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, encoding: u32) -> CFStringRef;
        fn CFRelease(obj: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(kind: CFStringRef, level: u32, name: CFStringRef, id: *mut u32) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    fn cfstr(s: &str) -> CFStringRef {
        let c = CString::new(s).expect("no NUL in constant");
        unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
    }

    #[derive(Default)]
    pub struct Hold {
        id: Option<u32>,
    }

    impl Hold {
        pub fn acquire(&mut self) {
            if self.id.is_some() {
                return;
            }
            let kind = cfstr("PreventUserIdleSystemSleep");
            let name = cfstr("Veltrix: Downloads laufen");
            let mut id = 0u32;
            let rc = unsafe { IOPMAssertionCreateWithName(kind, K_IOPM_ASSERTION_LEVEL_ON, name, &mut id) };
            unsafe {
                CFRelease(kind);
                CFRelease(name);
            }
            if rc == 0 {
                tracing::info!("preventing idle sleep while downloading");
                self.id = Some(id);
            } else {
                tracing::warn!(rc, "could not create power assertion");
            }
        }

        pub fn release(&mut self) {
            if let Some(id) = self.id.take() {
                unsafe { IOPMAssertionRelease(id) };
                tracing::info!("idle sleep allowed again");
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }

    #[derive(Default)]
    pub struct Hold {
        active: bool,
    }

    impl Hold {
        pub fn acquire(&mut self) {
            if !self.active && unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) } != 0 {
                tracing::info!("preventing idle sleep while downloading");
                self.active = true;
            }
        }

        pub fn release(&mut self) {
            if self.active {
                unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
                self.active = false;
                tracing::info!("idle sleep allowed again");
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    #[derive(Default)]
    pub struct Hold;

    impl Hold {
        pub fn acquire(&mut self) {}
        pub fn release(&mut self) {}
    }
}
