//! Elevation. Packet capture via pktmon needs administrator rights.
//! Relaunch logic adapted from irminsul `admin.rs` (MIT, (c) 2024 IceDynamix).

#[cfg(windows)]
pub fn is_elevated() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().into() }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Relaunch this executable with the same arguments through the UAC prompt.
/// Returns only on failure.
#[cfg(windows)]
pub fn relaunch_elevated() -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_OWNER, SW_SHOWNORMAL};

    let args = std::env::args()
        .skip(1)
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let exe: Vec<u16> = std::env::current_exe()?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let args: Vec<u16> = args.encode_utf16().chain(Some(0)).collect();

    unsafe {
        let console = GetConsoleWindow();
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE,
            hwnd: GetWindow(console, GW_OWNER).unwrap_or(console),
            lpVerb: w!("runas"),
            lpFile: PCWSTR(exe.as_ptr()),
            lpParameters: PCWSTR(args.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        ShellExecuteExW(&mut info)?;
    }
    std::process::exit(0);
}

#[cfg(not(windows))]
pub fn relaunch_elevated() -> anyhow::Result<()> {
    anyhow::bail!("elevation is only implemented on Windows")
}
