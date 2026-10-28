//! OS-level enforcement. Windows uses Win32 directly; other platforms
//! currently monitor only (enforcement is a no-op).

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            ProcessStatus::K32EmptyWorkingSet,
            Threading::{
                OpenProcess, SetPriorityClass, BELOW_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA,
            },
        },
    };

    fn with_process(pid: u32, f: impl FnOnce(windows_sys::Win32::Foundation::HANDLE)) {
        unsafe {
            let h = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() { return; }
            f(h);
            CloseHandle(h);
        }
    }

    pub fn set_low_priority(pid: u32, low: bool) {
        with_process(pid, |h| unsafe {
            SetPriorityClass(h, if low { BELOW_NORMAL_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS });
        });
    }

    pub fn trim(pid: u32) {
        with_process(pid, |h| unsafe { K32EmptyWorkingSet(h); });
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn set_low_priority(_pid: u32, _low: bool) {}
    pub fn trim(_pid: u32) {}
}

pub use imp::{set_low_priority, trim};
