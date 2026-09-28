pub(crate) use super::*;

use std::{
    io::Read,
    process::{Child, Command as StdCommand, Output, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub(crate) fn resolve_artifact_path(root: Option<&Path>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else if let Some(root) = root {
        root.join(path)
    } else {
        path
    }
}

pub(crate) fn resolve_verified_artifact_path(
    root: Option<&Path>,
    path: &str,
) -> io::Result<PathBuf> {
    let relative = PathBuf::from(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "artifact paths must stay inside the configured artifact root",
        ));
    }

    let base = fs::canonicalize(root.unwrap_or_else(|| Path::new(".")))?;
    let candidate = fs::canonicalize(base.join(relative))?;
    if !candidate.starts_with(&base) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "artifact path must stay inside the configured artifact root",
        ));
    }

    Ok(candidate)
}

#[allow(dead_code)]
pub(crate) async fn run_espflash_with_program(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
) -> Result<(), HttpError> {
    run_espflash_with_reset_fallback_with_program_and_identity(
        program,
        artifact,
        port_path,
        None,
        |before_reset| build_espflash_args_with_reset_mode(artifact, root, port_path, before_reset),
    )
    .await
}

pub(crate) async fn run_espflash_with_program_and_identity(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
    usb_identity: Option<&UsbSerialIdentity>,
) -> Result<(), HttpError> {
    run_espflash_with_reset_fallback_with_program_and_identity(
        program,
        artifact,
        port_path,
        usb_identity,
        |before_reset| build_espflash_args_with_reset_mode(artifact, root, port_path, before_reset),
    )
    .await
}

#[allow(dead_code)]
pub(crate) async fn run_espflash_with_reset_fallback_with_program<F>(
    program: &Path,
    artifact: &FirmwareArtifact,
    port_path: &str,
    build_commands: F,
) -> Result<(), HttpError>
where
    F: Fn(&str) -> Result<Vec<Vec<String>>, HttpError>,
{
    run_espflash_with_reset_fallback_with_program_and_identity(
        program,
        artifact,
        port_path,
        None,
        build_commands,
    )
    .await
}

#[derive(Clone, Copy)]
struct EspflashTarget<'a> {
    port_path: &'a str,
    usb_identity: Option<&'a UsbSerialIdentity>,
}

async fn run_espflash_with_reset_fallback_with_program_and_identity<F>(
    program: &Path,
    artifact: &FirmwareArtifact,
    port_path: &str,
    usb_identity: Option<&UsbSerialIdentity>,
    build_commands: F,
) -> Result<(), HttpError>
where
    F: Fn(&str) -> Result<Vec<Vec<String>>, HttpError>,
{
    let target = EspflashTarget {
        port_path,
        usb_identity,
    };
    let reset_modes = espflash_reset_modes(artifact, port_path);
    for (mode_index, before_reset) in reset_modes.iter().enumerate() {
        let commands = build_commands(before_reset)?;
        let mut retry_with_next_reset = false;

        for args in commands {
            require_usb_serial_identity(target.port_path, target.usb_identity)?;
            let output =
                run_espflash_command_with_target(program, &args, ESPFLASH_COMMAND_TIMEOUT, target)
                    .await?;
            require_usb_serial_identity(target.port_path, target.usb_identity)?;

            if output.status.success() {
                continue;
            }
            retry_with_next_reset = handle_failed_espflash_attempt(
                program,
                artifact,
                target,
                before_reset,
                &args,
                &output,
                mode_index + 1 < reset_modes.len(),
            )
            .await?;
            if retry_with_next_reset {
                break;
            }
        }

        if !retry_with_next_reset {
            return Ok(());
        }
    }

    Err(HttpError::internal(
        "espflash did not complete a reset attempt.",
    ))
}

async fn handle_failed_espflash_attempt(
    program: &Path,
    artifact: &FirmwareArtifact,
    target: EspflashTarget<'_>,
    before_reset: &str,
    args: &[String],
    output: &Output,
    can_retry: bool,
) -> Result<bool, HttpError> {
    if espflash_flash_end_requires_reset(args, output) {
        require_usb_serial_identity(target.port_path, target.usb_identity)?;
        let reset_args = build_espflash_reset_args(artifact, target.port_path, before_reset)?;
        let reset_output = run_espflash_command_with_target(
            program,
            &reset_args,
            ESPFLASH_COMMAND_TIMEOUT,
            target,
        )
        .await?;
        require_usb_serial_identity(target.port_path, target.usb_identity)?;
        if reset_output.status.success() {
            // The ROM accepted the image data but rejected the final
            // run-user-code transition. Reset once, then verify runtime_ready.
            return Ok(false);
        }
        return Err(HttpError::internal_with_details(
            "flash_recovery_reset_failed",
            "espflash reached FlashEnd but the recovery reset failed.",
            json!({
                "flashAttempt": espflash_failure_details(program, args, output),
                "resetAttempt": espflash_failure_details(program, &reset_args, &reset_output),
            }),
        ));
    }
    if !can_retry || !espflash_connection_failed(output) {
        return Err(HttpError::internal_with_details(
            "flash_tool_failed",
            "espflash returned a non-zero status.",
            espflash_failure_details(program, args, output),
        ));
    }
    if is_esp_usb_serial_jtag_port(target.port_path) {
        tokio::time::sleep(ESPFLASH_USB_RESET_RETRY_DELAY).await;
    }
    Ok(true)
}

#[allow(dead_code)]
pub(crate) async fn run_espflash_command_with_timeout(
    program: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Output, HttpError> {
    run_espflash_command_with_identity_blocking(program, args, timeout, None, None).await
}

async fn run_espflash_command_with_target(
    program: &Path,
    args: &[String],
    timeout: Duration,
    target: EspflashTarget<'_>,
) -> Result<Output, HttpError> {
    run_espflash_command_with_identity_blocking(
        program,
        args,
        timeout,
        Some(target.port_path),
        target.usb_identity,
    )
    .await
}

pub(crate) async fn run_espflash_command_with_identity(
    program: &Path,
    args: &[String],
    timeout: Duration,
    port_path: &str,
    expected_usb_identity: Option<&UsbSerialIdentity>,
) -> Result<Output, HttpError> {
    run_espflash_command_with_identity_blocking(
        program,
        args,
        timeout,
        Some(port_path),
        expected_usb_identity,
    )
    .await
}

async fn run_espflash_command_with_identity_blocking(
    program: &Path,
    args: &[String],
    timeout: Duration,
    port_path: Option<&str>,
    expected_usb_identity: Option<&UsbSerialIdentity>,
) -> Result<Output, HttpError> {
    let program = program.to_owned();
    let args = args.to_owned();
    let port_path = port_path.map(str::to_owned);
    let expected_usb_identity = expected_usb_identity.cloned();
    let control = Arc::new(EspflashProcessControl::default());
    let worker_control = Arc::clone(&control);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let worker = thread::spawn(move || {
        let result = run_espflash_command_blocking_with_control(
            &program,
            &args,
            timeout,
            port_path.as_deref(),
            expected_usb_identity.as_ref(),
            &worker_control,
        );
        let _ = sender.send(result);
    });
    let _guard = EspflashProcessGuard {
        control,
        worker: Some(worker),
    };
    receiver.await.map_err(|error| {
        HttpError::internal_with_details(
            "flash_tool_failed",
            "The espflash worker stopped unexpectedly.",
            json!({ "error": error.to_string() }),
        )
    })?
}

#[derive(Default)]
struct EspflashProcessControl {
    child: Mutex<Option<Child>>,
    cancelled: AtomicBool,
}

struct EspflashProcessGuard {
    control: Arc<EspflashProcessControl>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Drop for EspflashProcessGuard {
    fn drop(&mut self) {
        self.control.cancelled.store(true, Ordering::Release);
        if let Ok(mut child) = self.control.child.lock()
            && let Some(mut child) = child.take()
        {
            let _ = kill_espflash_process(&mut child);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_espflash_command_blocking_with_control(
    program: &Path,
    args: &[String],
    timeout: Duration,
    port_path: Option<&str>,
    expected_usb_identity: Option<&UsbSerialIdentity>,
    control: &Arc<EspflashProcessControl>,
) -> Result<Output, HttpError> {
    let mut child = spawn_espflash_process(program, args).map_err(|error| {
        HttpError::internal_with_details(
            "flash_tool_unavailable",
            "Failed to start espflash.",
            json!({ "program": program, "error": error.to_string() }),
        )
    })?;
    let stdout = child.stdout.take().expect("espflash stdout is piped");
    let stderr = child.stderr.take().expect("espflash stderr is piped");
    {
        let mut slot = control
            .child
            .lock()
            .map_err(|_| HttpError::internal("The espflash process state lock failed."))?;
        *slot = Some(child);
    }
    let stdout_reader = thread::spawn(move || read_process_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_process_pipe(stderr));
    let deadline = Instant::now() + timeout;

    loop {
        if control.cancelled.load(Ordering::Acquire) {
            kill_controlled_child(control);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(HttpError::internal("espflash operation was cancelled"));
        }
        let status = {
            let mut slot = control
                .child
                .lock()
                .map_err(|_| HttpError::internal("The espflash process state lock failed."))?;
            let child = slot
                .as_mut()
                .ok_or_else(|| HttpError::internal("The espflash process was cancelled."))?;
            child.try_wait()
        };
        match status {
            Ok(Some(status)) => {
                let _ = control.child.lock().map(|mut slot| slot.take());
                return collect_process_output(status, stdout_reader, stderr_reader);
            }
            Ok(None) => {}
            Err(error) => {
                return Err(controlled_inspection_error(
                    control,
                    stdout_reader,
                    stderr_reader,
                    program,
                    error,
                ));
            }
        }
        if let (Some(port_path), Some(expected_usb_identity)) = (port_path, expected_usb_identity)
            && !serial_port_usb_identity_matches(port_path, expected_usb_identity)
        {
            kill_controlled_child(control);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(HttpError::forbidden(
                "authorized_port_changed",
                "The authorized USB target changed or disappeared while espflash was running; the operation was stopped.",
            ));
        }
        if Instant::now() >= deadline {
            return Err(controlled_timeout_error(
                control,
                stdout_reader,
                stderr_reader,
                program,
                args,
                timeout,
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn controlled_inspection_error(
    control: &Arc<EspflashProcessControl>,
    stdout_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    program: &Path,
    error: io::Error,
) -> HttpError {
    let details = match kill_and_collect_controlled_child(control, stdout_reader, stderr_reader) {
        Ok(output) => json!({
            "program": program,
            "error": error.to_string(),
            "stdout": bounded_espflash_output(&output.stdout),
            "stderr": bounded_espflash_output(&output.stderr),
        }),
        Err(cleanup_error) => json!({
            "program": program,
            "error": error.to_string(),
            "cleanupError": cleanup_error.error.message,
        }),
    };
    HttpError::internal_with_details(
        "flash_tool_failed",
        "Failed to inspect the espflash process.",
        details,
    )
}

fn controlled_timeout_error(
    control: &Arc<EspflashProcessControl>,
    stdout_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    program: &Path,
    args: &[String],
    timeout: Duration,
) -> HttpError {
    let details = match kill_and_collect_controlled_child(control, stdout_reader, stderr_reader) {
        Ok(output) => json!({
            "program": program,
            "args": args,
            "timeoutMs": timeout.as_millis(),
            "stdout": bounded_espflash_output(&output.stdout),
            "stderr": bounded_espflash_output(&output.stderr),
        }),
        Err(cleanup_error) => json!({
            "program": program,
            "args": args,
            "timeoutMs": timeout.as_millis(),
            "cleanupError": cleanup_error.error.message,
        }),
    };
    HttpError::internal_with_details(
        "flash_tool_timeout",
        "espflash did not finish before the command deadline.",
        details,
    )
}

fn kill_controlled_child(control: &Arc<EspflashProcessControl>) {
    let _ = take_and_kill_controlled_child(control);
}

fn kill_and_collect_controlled_child(
    control: &Arc<EspflashProcessControl>,
    stdout_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
) -> Result<Output, HttpError> {
    let status = take_and_kill_controlled_child(control).map_err(|error| {
        HttpError::internal_with_details(
            "flash_tool_failed",
            "Failed to reap the espflash process.",
            json!({ "error": error.to_string() }),
        )
    })?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| HttpError::internal("The espflash stdout reader stopped unexpectedly."))?
        .map_err(|error| {
            HttpError::internal_with_details(
                "flash_tool_failed",
                "Failed to read espflash stdout.",
                json!({ "error": error.to_string() }),
            )
        })?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| HttpError::internal("The espflash stderr reader stopped unexpectedly."))?
        .map_err(|error| {
            HttpError::internal_with_details(
                "flash_tool_failed",
                "Failed to read espflash stderr.",
                json!({ "error": error.to_string() }),
            )
        })?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn take_and_kill_controlled_child(
    control: &Arc<EspflashProcessControl>,
) -> io::Result<std::process::ExitStatus> {
    let mut slot = control
        .child
        .lock()
        .map_err(|_| io::Error::other("espflash process state lock failed"))?;
    let mut child = slot
        .take()
        .ok_or_else(|| io::Error::other("espflash process is no longer available"))?;
    kill_espflash_process(&mut child)
}

pub fn run_espflash_command_blocking_with_identity(
    program: &Path,
    args: &[String],
    timeout: Duration,
    port_path: &str,
    expected_usb_identity: Option<&UsbSerialIdentity>,
) -> Result<Output, HttpError> {
    run_espflash_command_blocking(
        program,
        args,
        timeout,
        Some(port_path),
        expected_usb_identity,
    )
}

fn run_espflash_command_blocking(
    program: &Path,
    args: &[String],
    timeout: Duration,
    port_path: Option<&str>,
    expected_usb_identity: Option<&UsbSerialIdentity>,
) -> Result<Output, HttpError> {
    let mut child = spawn_espflash_process(program, args).map_err(|error| {
        HttpError::internal_with_details(
            "flash_tool_unavailable",
            "Failed to start espflash.",
            json!({ "program": program, "error": error.to_string() }),
        )
    })?;
    let stdout = child.stdout.take().expect("espflash stdout is piped");
    let stderr = child.stderr.take().expect("espflash stderr is piped");
    let stdout_reader = thread::spawn(move || read_process_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_process_pipe(stderr));
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return collect_process_output(status, stdout_reader, stderr_reader);
            }
            Ok(None) => {}
            Err(error) => {
                let details =
                    match kill_and_collect_process_output(&mut child, stdout_reader, stderr_reader)
                    {
                        Ok(output) => json!({
                            "program": program,
                            "error": error.to_string(),
                            "stdout": bounded_espflash_output(&output.stdout),
                            "stderr": bounded_espflash_output(&output.stderr),
                        }),
                        Err(cleanup_error) => json!({
                            "program": program,
                            "error": error.to_string(),
                            "cleanupError": cleanup_error.error.message,
                        }),
                    };
                return Err(HttpError::internal_with_details(
                    "flash_tool_failed",
                    "Failed to inspect the espflash process.",
                    details,
                ));
            }
        }
        if let (Some(port_path), Some(expected_usb_identity)) = (port_path, expected_usb_identity)
            && !serial_port_usb_identity_matches(port_path, expected_usb_identity)
        {
            let _ = kill_espflash_process(&mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(HttpError::forbidden(
                "authorized_port_changed",
                "The authorized USB target changed or disappeared while espflash was running; the operation was stopped.",
            ));
        }
        if Instant::now() >= deadline {
            let details =
                match kill_and_collect_process_output(&mut child, stdout_reader, stderr_reader) {
                    Ok(output) => json!({
                        "program": program,
                        "args": args,
                        "timeoutMs": timeout.as_millis(),
                        "stdout": bounded_espflash_output(&output.stdout),
                        "stderr": bounded_espflash_output(&output.stderr),
                    }),
                    Err(cleanup_error) => json!({
                        "program": program,
                        "args": args,
                        "timeoutMs": timeout.as_millis(),
                        "cleanupError": cleanup_error.error.message,
                    }),
                };
            return Err(HttpError::internal_with_details(
                "flash_tool_timeout",
                "espflash did not finish before the command deadline.",
                details,
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn kill_and_collect_process_output(
    child: &mut Child,
    stdout_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
) -> Result<Output, HttpError> {
    let status = kill_espflash_process(child).map_err(|error| {
        HttpError::internal_with_details(
            "flash_tool_failed",
            "Failed to reap the espflash process.",
            json!({ "error": error.to_string() }),
        )
    })?;
    collect_process_output(status, stdout_reader, stderr_reader)
}

fn spawn_espflash_process(program: &Path, args: &[String]) -> io::Result<Child> {
    let mut command = StdCommand::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    command.spawn()
}

fn kill_espflash_process(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as libc::pid_t);
        let result = unsafe { libc::kill(process_group, libc::SIGKILL) };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                let _ = child.kill();
            }
        }
    }
    #[cfg(windows)]
    {
        // Windows has no std process-group kill; taskkill's tree mode closes
        // inherited stdout/stderr handles held by normal child processes.
        let pid = child.id().to_string();
        let taskkill = StdCommand::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .status();
        if taskkill.map_or(true, |status| !status.success()) {
            let _ = child.kill();
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = child.kill();
    child.wait()
}

pub(crate) const MAX_ESPFLASH_CAPTURE_BYTES: usize = 64 * 1024;

pub(crate) fn read_process_pipe<R: Read>(mut reader: R) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(MAX_ESPFLASH_CAPTURE_BYTES);
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_ESPFLASH_CAPTURE_BYTES.saturating_sub(output.len());
        if remaining > 0 {
            output.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    }
    Ok(output)
}

fn collect_process_output(
    status: std::process::ExitStatus,
    stdout_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: thread::JoinHandle<io::Result<Vec<u8>>>,
) -> Result<Output, HttpError> {
    let stdout = stdout_reader
        .join()
        .map_err(|_| HttpError::internal("The espflash stdout reader stopped unexpectedly."))?
        .map_err(|error| {
            HttpError::internal_with_details(
                "flash_tool_failed",
                "Failed to read espflash stdout.",
                json!({ "error": error.to_string() }),
            )
        })?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| HttpError::internal("The espflash stderr reader stopped unexpectedly."))?
        .map_err(|error| {
            HttpError::internal_with_details(
                "flash_tool_failed",
                "Failed to read espflash stderr.",
                json!({ "error": error.to_string() }),
            )
        })?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn build_espflash_reset_args(
    artifact: &FirmwareArtifact,
    port_path: &str,
    before_reset: &str,
) -> Result<Vec<String>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    Ok(vec![
        "reset".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "hard-reset".to_string(),
        "--non-interactive".to_string(),
    ])
}

pub(crate) fn build_bundle_write_bin_args(
    common: &[String],
    before_reset: &str,
    address: u64,
    input_path: &Path,
) -> Vec<String> {
    let mut args = vec!["write-bin".to_string()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        input_path.to_string_lossy().into_owned(),
    ]);
    args
}

#[allow(dead_code)]
pub(crate) async fn run_flash_transaction_with_program(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
) -> Result<(), HttpError> {
    run_espflash_with_program(artifact, root, port_path, program).await
}

pub(crate) async fn run_flash_transaction_with_program_and_identity(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
    usb_identity: Option<&UsbSerialIdentity>,
) -> Result<(), HttpError> {
    run_espflash_with_program_and_identity(artifact, root, port_path, program, usb_identity).await
}

pub(crate) fn resolve_espflash_program() -> PathBuf {
    if let Some(program) = env::var_os("FLUX_PURR_ESPFLASH").filter(|value| !value.is_empty()) {
        return PathBuf::from(program);
    }
    if let Some(cargo_home) = env::var_os("CARGO_HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(cargo_home).join("bin").join("espflash");
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join("espflash");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("espflash")
}

pub(crate) fn espflash_failure_details(program: &Path, args: &[String], output: &Output) -> Value {
    json!({
        "program": program,
        "args": args,
        "exitCode": output.status.code(),
        "stdout": bounded_espflash_output(&output.stdout),
        "stderr": bounded_espflash_output(&output.stderr),
    })
}

pub(crate) fn espflash_connection_failed(output: &Output) -> bool {
    espflash_connection_failure_text(&bounded_espflash_output(&output.stderr))
        || espflash_connection_failure_text(&bounded_espflash_output(&output.stdout))
}

pub(crate) fn espflash_flash_end_requires_reset(args: &[String], output: &Output) -> bool {
    args.first().map(String::as_str) == Some("flash")
        && bounded_espflash_output(&output.stderr).contains("Error while running FlashEnd command")
}

pub(crate) fn espflash_connection_failure_text(output: &str) -> bool {
    let output = output.to_ascii_lowercase();
    output.contains("failed to connect to the device")
        || output.contains("error while connecting to device")
        || output.contains("no such device or address")
        || output.contains("broken pipe")
}

pub(crate) fn bounded_espflash_output(bytes: &[u8]) -> String {
    const MAX_OUTPUT_BYTES: usize = 4_096;
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MAX_OUTPUT_BYTES {
        return text.trim().to_string();
    }
    let mut end = MAX_OUTPUT_BYTES;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{} [truncated]", text[..end].trim())
}

pub(crate) async fn run_espflash_with_exclusive_serial(
    state: &AppState,
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    usb_identity: Option<&UsbSerialIdentity>,
) -> Result<(), HttpError> {
    let _serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let _serial_lock = acquire_serial_process_lock(port_path, ESPFLASH_COMMAND_TIMEOUT).await?;
    let program = resolve_espflash_program();
    run_flash_transaction_with_program_and_identity(
        artifact,
        root,
        port_path,
        &program,
        usb_identity,
    )
    .await
}

pub(crate) async fn acquire_serial_rpc_with_timeout(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    timeout: Duration,
) -> Result<tokio::sync::OwnedMutexGuard<()>, HttpError> {
    tokio::time::timeout(timeout, serial_rpc.lock_owned())
        .await
        .map_err(|_| {
            HttpError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "serial_lock_timeout",
                "Timed out waiting for exclusive USB serial access.",
                true,
            )
        })
}

pub(crate) fn drop_cached_serial_session(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
) -> Result<(), HttpError> {
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    remove_cached_serial_session(&mut serial_sessions, port_path);
    Ok(())
}

pub(crate) fn build_espflash_args_with_reset_mode(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    before_reset: &str,
) -> Result<Vec<Vec<String>>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    let partition_table = firmware_partition_table_path(root)?;
    if let Some(elf_image) = artifact.files.iter().find(|file| file.kind == "elf") {
        let path = resolve_artifact_path(root, &elf_image.path);
        let mut args = vec![
            "flash".to_string(),
            "--chip".to_string(),
            artifact.target_chip.clone(),
            "--port".to_string(),
            port_path.to_string(),
            "--before".to_string(),
            before_reset.to_string(),
            "--non-interactive".to_string(),
            "--no-stub".to_string(),
            "--after".to_string(),
            "hard-reset".to_string(),
        ];
        args.push("--partition-table".to_string());
        args.push(partition_table.to_string_lossy().into_owned());
        args.push(path.to_string_lossy().into_owned());
        return Ok(vec![args]);
    }

    let Some(app_image) = artifact.files.iter().find(|file| file.kind == "app") else {
        return Err(HttpError::bad_request(
            "missing_flash_image",
            "Artifact does not contain an ELF or raw app image.",
        ));
    };
    let app_address = app_image.flash_address.ok_or_else(|| {
        HttpError::bad_request("missing_flash_address", "Missing app flash address.")
    })?;
    let partition_table_binary = firmware_partition_table_binary_path(root)?;
    let app_path = resolve_artifact_path(root, &app_image.path);
    let common = vec![
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--non-interactive".to_string(),
    ];
    let mut partition_table_args = vec!["write-bin".to_string()];
    partition_table_args.extend(common.clone());
    partition_table_args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        DEFAULT_PARTITION_TABLE_FLASH_ADDRESS.to_string(),
        partition_table_binary.to_string_lossy().into_owned(),
    ]);
    let mut app_args = vec!["write-bin".to_string()];
    app_args.extend(common.clone());
    app_args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        app_address.to_string(),
        app_path.to_string_lossy().into_owned(),
    ]);
    // espflash write-bin leaves the target in its loader. Reset explicitly once both images land.
    let mut reset_args = vec!["reset".to_string()];
    reset_args.extend(common);
    reset_args.extend(["--before".to_string(), before_reset.to_string()]);
    Ok(vec![partition_table_args, app_args, reset_args])
}

pub(crate) fn firmware_partition_table_path(root: Option<&Path>) -> Result<PathBuf, HttpError> {
    let Some(root) = root else {
        return Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Firmware flashing requires an artifact root containing firmware/partitions.csv.",
        ));
    };
    let partition_table = root.join("firmware/partitions.csv");
    if partition_table.is_file() {
        Ok(partition_table)
    } else {
        Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Firmware flashing requires firmware/partitions.csv for the partition table.",
        ))
    }
}

pub(crate) fn firmware_partition_table_binary_path(
    root: Option<&Path>,
) -> Result<PathBuf, HttpError> {
    let Some(root) = root else {
        return Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Raw app flashing requires an artifact root containing firmware/partitions.bin.",
        ));
    };
    let partition_table = root.join("firmware/partitions.bin");
    if partition_table.is_file() {
        Ok(partition_table)
    } else {
        Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Raw app flashing requires firmware/partitions.bin for the partition table.",
        ))
    }
}
