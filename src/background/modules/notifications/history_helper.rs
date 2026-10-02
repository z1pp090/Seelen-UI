//! Toast history lookup from outside of the MSIX package.
//!
//! When Seelen runs packaged, `ToastNotificationHistory::GetHistory` returns an empty list for
//! the AUMIDs of non packaged (win32) apps, so the toast XML of those apps (launch arguments,
//! actions, images) can't be read and clicking their notifications does nothing.
//!
//! Child processes of a packaged desktop app run outside of the package by default, and from
//! there the same call works. So in that case we spawn our own executable with [`HELPER_ARG`],
//! it prints the XML of every toast in the history of the app as JSON and exits.

use std::{
    io::Write,
    os::windows::process::CommandExt,
    process::{Command, ExitCode, Stdio},
};

use windows::{
    UI::Notifications::ToastNotificationManager, Win32::System::Threading::CREATE_NO_WINDOW,
};

use crate::{error::Result, utils::is_running_as_appx};

const HELPER_ARG: &str = "--toast-history-helper";

/// Should be called at the very start of `main`, before any other initialization.
/// Returns `Some` if this process was spawned as the helper, the caller should exit with it.
pub fn run_if_requested() -> Option<ExitCode> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(HELPER_ARG) {
        return None;
    }

    let Some(umid) = args.next() else {
        return Some(ExitCode::from(2));
    };

    // running inside the package would give the same empty result as the caller
    if is_running_as_appx() {
        return Some(ExitCode::from(3));
    }

    let Ok(xmls) = read_toast_history(&umid) else {
        return Some(ExitCode::FAILURE);
    };

    let Ok(json) = serde_json::to_vec(&xmls) else {
        return Some(ExitCode::FAILURE);
    };

    match std::io::stdout().write_all(&json) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(_) => Some(ExitCode::FAILURE),
    }
}

fn read_toast_history(umid: &str) -> Result<Vec<String>> {
    let history = ToastNotificationManager::History()?;
    let mut xmls = Vec::new();
    for toast in history.GetHistoryWithId(&umid.into())? {
        // this can be null when the notification count is bigger than the max allowed by default 20
        if let Ok(content) = toast.Content() {
            xmls.push(content.GetXml()?.to_string());
        }
    }
    Ok(xmls)
}

/// Returns the XML of every toast in the history of the given win32 app, read from a child
/// process running outside of the package.
pub fn get_toast_history_outside_package(umid: &str) -> Result<Vec<String>> {
    let output = Command::new(std::env::current_exe()?)
        .arg(HELPER_ARG)
        .arg(umid)
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;

    if !output.status.success() {
        return Err(format!("Toast history helper failed with {}", output.status).into());
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}
