//! Lower Aether / PresentMon scheduling priority while a game is active so
//! frame pacing (esp. Vulkan + DWM) stays smooth.

use std::sync::atomic::{AtomicBool, Ordering};

static IN_GAME: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
fn set_current_priority(idle: bool) {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
    };
    unsafe {
        let _ = SetPriorityClass(
            GetCurrentProcess(),
            if idle {
                IDLE_PRIORITY_CLASS
            } else {
                NORMAL_PRIORITY_CLASS
            },
        );
    }
}

#[cfg(not(windows))]
fn set_current_priority(_idle: bool) {}

#[cfg(windows)]
pub fn set_pid_idle(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, SetPriorityClass, IDLE_PRIORITY_CLASS, PROCESS_SET_INFORMATION,
    };
    if pid == 0 {
        return;
    }
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SET_INFORMATION, false, pid) {
            let _ = SetPriorityClass(h, IDLE_PRIORITY_CLASS);
            let _ = CloseHandle(h);
        }
    }
}

#[cfg(not(windows))]
pub fn set_pid_idle(_pid: u32) {}

pub fn set_gaming(active: bool) {
    let prev = IN_GAME.swap(active, Ordering::SeqCst);
    if prev == active {
        return;
    }
    // IDLE while gaming — Aether must never compete with the game's frame budget.
    set_current_priority(active);
}

pub fn is_gaming() -> bool {
    IN_GAME.load(Ordering::Relaxed)
}
