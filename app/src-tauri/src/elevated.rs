//! The Windows fast scan with administrator rights, without running the
//! window as administrator.
//!
//! Reading a drive's file table needs administrator rights. Rather than
//! elevating the whole app (and its web view), Strata starts a second copy
//! of itself through the Windows permission prompt. That copy only reads
//! the file table of one drive, writes the result to a new file in Strata's
//! own data folder, and exits. It opens no window and takes no other
//! instructions; its arguments are checked strictly because they cross a
//! privilege boundary.

/// The argument that starts the elevated copy.
pub const FLAG: &str = "--strata-elevated-scan";

#[cfg(windows)]
#[derive(Debug)]
pub enum ElevatedError {
    /// The person said no at the Windows permission prompt.
    Declined,
    /// The elevated copy ran and reported a problem.
    Failed(String),
}

#[cfg(windows)]
impl std::fmt::Display for ElevatedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElevatedError::Declined => f.write_str("administrator access was not allowed"),
            ElevatedError::Failed(why) => f.write_str(why),
        }
    }
}

/// Exit codes of the elevated copy.
#[cfg_attr(not(windows), allow(dead_code))]
mod code {
    pub const OK: i32 = 0;
    pub const BAD_ARGUMENTS: i32 = 2;
    pub const NOT_ELEVATED: i32 = 3;
    pub const NOT_NTFS: i32 = 4;
    pub const REFUSED: i32 = 5;
    pub const FAILED: i32 = 6;
}

#[cfg(windows)]
pub use imp::{run_elevated_copy, scan_drive_elevated};

#[cfg(not(windows))]
pub fn run_elevated_copy(_args: &[String]) -> i32 {
    code::BAD_ARGUMENTS
}

#[cfg(windows)]
mod imp {
    use super::{code, ElevatedError, FLAG};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use strata_classify::KindClassifier;
    use strata_core::{ScanControl, ScanTree};
    use strata_mft::{open_drive, scan_volume, MftError, MftOptions};
    use strata_store::{ScanMeta, Store};
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NO_CONSOLE,
        SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    /// Strata's data folder, as Tauri places it: %APPDATA%\<identifier>.
    fn data_folder() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("app.strata.desktop"))
    }

    /// The only output files the elevated copy will write.
    fn acceptable_output(path: &Path) -> bool {
        let (Some(folder), Some(parent), Some(name)) =
            (data_folder(), path.parent(), path.file_name())
        else {
            return false;
        };
        let name = name.to_string_lossy();
        let digits = name
            .strip_prefix("fast-scan-")
            .and_then(|n| n.strip_suffix(".sqlite"));
        parent == folder
            && digits.is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
    }

    /// The elevated copy: `strata-app --strata-elevated-scan <letter> <output>`.
    pub fn run_elevated_copy(args: &[String]) -> i32 {
        let [_, flag, letter, output] = args else {
            return code::BAD_ARGUMENTS;
        };
        let mut chars = letter.chars();
        let (Some(letter), None) = (chars.next(), chars.next()) else {
            return code::BAD_ARGUMENTS;
        };
        let output = PathBuf::from(output);
        if flag != FLAG || !letter.is_ascii_alphabetic() || !acceptable_output(&output) {
            return code::BAD_ARGUMENTS;
        }
        if !strata_mft::is_elevated() {
            return code::NOT_ELEVATED;
        }
        // A fresh file only: never follow or overwrite something already there.
        if std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .is_err()
        {
            return code::BAD_ARGUMENTS;
        }
        let tree = match open_drive(letter).and_then(|drive| {
            let name = format!("{}:\\", letter.to_ascii_uppercase());
            scan_volume(
                drive,
                &MftOptions::new(name),
                &KindClassifier,
                &ScanControl::new(),
            )
        }) {
            Ok(tree) => tree,
            Err(MftError::NotNtfs) => return code::NOT_NTFS,
            Err(MftError::NeedsAdministrator | MftError::AccessRefused(_)) => return code::REFUSED,
            Err(_) => return code::FAILED,
        };
        let meta = ScanMeta {
            volume_id: String::new(),
            started_at: 0,
            finished_at: 0,
        };
        match Store::open(&output).and_then(|mut s| s.save(&tree, &meta)) {
            Ok(_) => code::OK,
            Err(_) => code::FAILED,
        }
    }

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(Some(0)).collect()
    }

    /// Ask Windows to run the elevated copy for `letter`, wait for it, and
    /// read its result back. `data_folder` is this app's data folder.
    pub fn scan_drive_elevated(
        letter: char,
        data_folder: &Path,
    ) -> Result<ScanTree, ElevatedError> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let output = data_folder.join(format!("fast-scan-{}{stamp}.sqlite", std::process::id()));
        let exe = std::env::current_exe().map_err(|e| ElevatedError::Failed(e.to_string()))?;
        // Windows paths cannot contain quotes, so quoting is enough here.
        let parameters = format!(
            "{FLAG} {} \"{}\"",
            letter.to_ascii_uppercase(),
            output.display()
        );

        let verb = wide(OsStr::new("runas"));
        let file = wide(exe.as_os_str());
        let params = wide(OsStr::new(&parameters));
        // SAFETY: SHELLEXECUTEINFOW is plain data; all-zero is its documented
        // starting state before the fields below are set.
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;

        // SAFETY: `info` is initialised as above and the three strings it
        // points to outlive the call.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            // SAFETY: GetLastError has no preconditions.
            let error = unsafe { GetLastError() };
            let _ = std::fs::remove_file(&output); // own-file: nothing was written yet
            return Err(if error == ERROR_CANCELLED {
                ElevatedError::Declined
            } else {
                ElevatedError::Failed(std::io::Error::from_raw_os_error(error as i32).to_string())
            });
        }
        let mut exit = 1u32;
        // SAFETY: SEE_MASK_NOCLOSEPROCESS makes `hProcess` a valid process
        // handle that this function owns and closes.
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            GetExitCodeProcess(info.hProcess, &mut exit);
            CloseHandle(info.hProcess);
        }

        let result = match exit as i32 {
            code::OK => read_result(&output),
            code::NOT_NTFS => Err(ElevatedError::Failed("the drive is not NTFS".into())),
            code::REFUSED => Err(ElevatedError::Failed(
                "Windows refused direct access to the drive even with administrator rights (security software can do this)".into(),
            )),
            other => Err(ElevatedError::Failed(format!("the fast scan stopped with code {other}"))),
        };
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut name = output.clone().into_os_string();
            name.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(name)); // own-file: the temporary result
        }
        result
    }

    fn read_result(output: &Path) -> Result<ScanTree, ElevatedError> {
        let failed = |e: strata_store::StoreError| ElevatedError::Failed(e.to_string());
        let store = Store::open(output).map_err(failed)?;
        let scan = store
            .scans(None)
            .map_err(failed)?
            .first()
            .map(|s| s.id)
            .ok_or_else(|| ElevatedError::Failed("the fast scan wrote no result".into()))?;
        store.load(scan).map_err(failed)
    }
}
