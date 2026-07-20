from .core import *

DEVICE_REFRESH_RETRY_DELAY_SECONDS = 1.0
MAX_DEVICE_REFRESH_ATTEMPTS = 3
TRANSIENT_TEMPERATURE_FAULTS = frozenset(
    {"sensor-glitch", "sensor-open", "sensor-short", "adc-read-failed"}
)
TRANSIENT_WARNING_CLEAR_POLL_ATTEMPTS = 20
TRANSIENT_WARNING_CLEAR_POLL_INTERVAL_SECONDS = 0.5


@dataclass
class SelfTestRun:
    output: dict[str, Any]
    summary: dict[str, Any]
    summary_path: Path
    run_dir: Path
    samples_path: Path


class AlarmInterventionRequired(RuntimeError):
    def __init__(self, attempts: list[dict[str, Any]]):
        self.attempts = attempts
        super().__init__(
            "three consecutive alarm-affected tests require manual inspection before continuing"
        )


class FluxPurrRunner:
    def __init__(
        self,
        flux_purr_bin: Path,
        devd_url: str,
        authorized_port: str,
        source_id: str,
        source_url: str,
        dry_run: bool,
        auto_recover_source: bool,
    ):
        self.flux_purr_bin = flux_purr_bin
        self.devd_url = devd_url.rstrip("/")
        self.authorized_port = authorized_port
        self.source_id = source_id
        self.source_url = source_url
        self.dry_run = dry_run
        self.auto_recover_source = auto_recover_source
        self._resolved_device_id: str | None = None
        self._resolved_source_id: str | None = None
        self._consecutive_alarm_attempts: list[dict[str, Any]] = []

    def _query_devd_devices(self) -> list[dict[str, Any]]:
        url = f"{self.devd_url}/api/v1/devices"
        req = urllib.request.Request(url, method="GET")
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                payload = json.loads(resp.read().decode())
        except urllib.error.URLError as exc:
            raise RuntimeError(f"failed to query devd devices at {url}: {exc}") from exc
        devices = payload.get("devices") if isinstance(payload, dict) else None
        if not isinstance(devices, list):
            raise RuntimeError("unexpected devd /devices payload")
        return [device for device in devices if isinstance(device, dict)]

    def _device_matches_authorized_port(self, device: dict[str, Any]) -> bool:
        return device.get("portPath") == self.authorized_port

    def _device_is_missing_port_placeholder(self, device: dict[str, Any]) -> bool:
        device_id = str(device.get("id") or "")
        return device_id.startswith("serial-_dev_")

    def run_subprocess_json(self, cmd: list[str]) -> dict[str, Any]:
        log(f"$ {shlex.join(cmd)}")
        proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
        if proc.returncode != 0:
            raise RuntimeError(
                f"command failed ({proc.returncode}): {shlex.join(cmd)}\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
            )
        if not proc.stdout.strip():
            raise RuntimeError(f"command produced empty stdout: {shlex.join(cmd)}")
        return json.loads(proc.stdout)

    def resolve_device_id(self, dry_run_override: bool = False) -> str:
        if self.dry_run or dry_run_override:
            return "mock-fp-lab-01"
        if self._resolved_device_id is not None:
            return self._resolved_device_id
        last_matches: list[dict[str, Any]] = []
        for attempt in range(MAX_DEVICE_REFRESH_ATTEMPTS):
            matches = [
                device
                for device in self._query_devd_devices()
                if self._device_matches_authorized_port(device)
            ]
            if not matches:
                if attempt + 1 < MAX_DEVICE_REFRESH_ATTEMPTS:
                    time.sleep(DEVICE_REFRESH_RETRY_DELAY_SECONDS)
                    continue
                break
            live_matches = [
                device for device in matches if not self._device_is_missing_port_placeholder(device)
            ]
            if len(live_matches) == 1:
                self._resolved_device_id = str(live_matches[0]["id"])
                return self._resolved_device_id
            if len(live_matches) > 1:
                raise RuntimeError(
                    f"expected exactly one live device on authorized port {self.authorized_port}, got {len(live_matches)}"
                )
            last_matches = matches
            if attempt + 1 < MAX_DEVICE_REFRESH_ATTEMPTS:
                time.sleep(DEVICE_REFRESH_RETRY_DELAY_SECONDS)
        if self._resolved_device_id is not None:
            return self._resolved_device_id
        if last_matches:
            raise RuntimeError(
                f"authorized port {self.authorized_port} only exposed missing-port placeholders; live bridge not ready"
            )
        raise RuntimeError(
            f"expected at least one device on authorized port {self.authorized_port}, got 0"
        )

    def acknowledge_fault_attention(self) -> dict[str, Any] | None:
        if self.dry_run:
            return None
        return self.run_json_command(
            [
                str(self.flux_purr_bin),
                "--devd",
                self.devd_url,
                "--json",
                "runtime",
                "set",
                "--device",
                self.resolve_device_id(False),
                "--fault-attention-acknowledged",
            ]
        )

    def resolve_source_id(self, dry_run_override: bool = False) -> str:
        if self.dry_run or dry_run_override:
            return self.source_id
        if self._resolved_source_id is not None:
            return self._resolved_source_id
        status = self.run_subprocess_json(
            ["isolapurr", "status", "--url", self.source_url, "--json"]
        )
        device = status.get("device") if isinstance(status, dict) else None
        actual_source_id = device.get("device_id") if isinstance(device, dict) else None
        if not isinstance(actual_source_id, str) or not actual_source_id:
            raise RuntimeError(
                f"isolapurr status at {self.source_url} did not return a device_id"
            )
        if self.source_id != actual_source_id and not actual_source_id.startswith(self.source_id):
            raise RuntimeError(
                "isolapurr identity mismatch "
                f"source_url={self.source_url} expected_device_id={self.source_id} "
                f"actual_device_id={actual_source_id}"
            )
        self._resolved_source_id = actual_source_id
        return self._resolved_source_id

    def recover_source_output(self) -> None:
        if self.dry_run:
            return
        log("source recovery: power-cycle IsolaPurr runtime output on the same authorized source")
        self.run_subprocess_json(
            [
                "isolapurr",
                "power",
                "runtime",
                "output",
                "--url",
                self.source_url,
                "--enabled",
                "false",
                "--json",
            ]
        )
        disconnect_error: RuntimeError | None = None
        for _ in range(SOURCE_RECOVERY_POLL_ATTEMPTS):
            disconnected = self.run_subprocess_json(
                ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
            )
            try:
                verify_isolapurr_output_disabled(disconnected)
                disconnect_error = None
                break
            except RuntimeError as exc:
                disconnect_error = exc
                time.sleep(SOURCE_RECOVERY_POLL_INTERVAL_SECONDS)
        if disconnect_error is not None:
            raise disconnect_error
        time.sleep(SOURCE_RECOVERY_SETTLE_SECONDS)
        self.run_subprocess_json(
            [
                "isolapurr",
                "power",
                "runtime",
                "output",
                "--url",
                self.source_url,
                "--enabled",
                "true",
                "--json",
            ]
        )
        baseline = self.run_subprocess_json(
            ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
        )
        previous_sample_uptime_ms = verify_isolapurr_power_show(
            baseline,
            expect_usb_c_enabled=True,
        )
        last_error: RuntimeError | None = None
        for _ in range(SOURCE_RECOVERY_POLL_ATTEMPTS):
            time.sleep(SOURCE_RECOVERY_POLL_INTERVAL_SECONDS)
            current = self.run_subprocess_json(
                ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
            )
            try:
                verify_isolapurr_power_show(
                    current,
                    expect_usb_c_enabled=True,
                    previous_sample_uptime_ms=previous_sample_uptime_ms,
                )
                if not Path(self.authorized_port).exists():
                    raise RuntimeError(
                        f"authorized port disappeared after source recovery: {self.authorized_port}"
                    )
                return
            except RuntimeError as exc:
                last_error = exc
                previous = source_usb_c_sample_uptime_ms(current)
                if previous is not None:
                    previous_sample_uptime_ms = previous
        raise last_error if last_error is not None else RuntimeError("source recovery did not restore live telemetry")

    def run_json_command(
        self,
        cmd: list[str],
        *,
        retry_with_source_recovery: bool = False,
    ) -> dict[str, Any]:
        allow_source_recovery = retry_with_source_recovery and self.auto_recover_source and not self.dry_run
        device_refresh_attempts = 0
        attempted_source_recovery = False
        current_cmd = list(cmd)
        while True:
            log(f"$ {shlex.join(current_cmd)}")
            proc = subprocess.run(current_cmd, cwd=REPO_ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                if not proc.stdout.strip():
                    raise RuntimeError(f"command produced empty stdout: {shlex.join(current_cmd)}")
                return json.loads(proc.stdout)
            error_text = (
                f"command failed ({proc.returncode}): {shlex.join(current_cmd)}\n"
                f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
            )
            if (
                device_refresh_attempts < MAX_DEVICE_REFRESH_ATTEMPTS
                and not self.dry_run
                and self._command_needs_device_refresh(error_text)
            ):
                refreshed_cmd = self._refresh_device_command(current_cmd)
                if refreshed_cmd is not None:
                    device_refresh_attempts += 1
                    current_cmd = refreshed_cmd
                    time.sleep(DEVICE_REFRESH_RETRY_DELAY_SECONDS)
                    continue
            if allow_source_recovery and not attempted_source_recovery:
                attempted_source_recovery = True
                self.recover_source_output()
                device_refresh_attempts = 0
                refreshed_cmd = self._refresh_device_command(current_cmd)
                current_cmd = refreshed_cmd or list(current_cmd)
                continue
            raise RuntimeError(error_text)

    def _command_needs_device_refresh(self, error_text: str) -> bool:
        return any(
            marker in error_text
            for marker in (
                "device_not_found",
                "saved hardware not found",
                "lease_conflict",
                "serial_reconnect_timeout",
            )
        )

    def _refresh_device_command(self, cmd: list[str]) -> list[str] | None:
        if "--device" not in cmd or self.dry_run:
            return None
        device_index = cmd.index("--device") + 1
        if device_index >= len(cmd):
            return None
        previous_device_id = self._resolved_device_id
        self._resolved_device_id = None
        refreshed = list(cmd)
        try:
            refreshed[device_index] = self.resolve_device_id(False)
        except RuntimeError:
            if previous_device_id is None:
                raise
            self._resolved_device_id = previous_device_id
            refreshed[device_index] = previous_device_id
        return refreshed

    def self_test(
        self,
        *,
        seed_profile_file: Path | None = None,
        candidate_profile_files: list[Path] | None = None,
        targets_c: list[int],
        hold_seconds: int,
        output_dir: Path,
        evaluation_mode: str = EVALUATION_MODE_HOLD_CONFIRM,
        cooldown_temp_c: float | None = None,
        stage_timeout_seconds: int | None = None,
        warmup_timeout_seconds: int | None = None,
        cooldown_timeout_seconds: int | None = None,
        dry_run_override: bool = False,
    ) -> SelfTestRun:
        dry_run = self.dry_run or dry_run_override
        attempt_dirs = [output_dir, output_dir.with_name(f"{output_dir.name}-rerun1")]
        last_result: SelfTestRun | None = None
        for attempt_index, attempt_dir in enumerate(attempt_dirs):
            attempt_dir.mkdir(parents=True, exist_ok=True)
            cmd = [
                str(self.flux_purr_bin),
                "--devd",
                self.devd_url,
                "--json",
                "thermal",
                "self-test",
                "--source-kind",
                SOURCE_KIND,
                "--source-id",
                self.resolve_source_id(dry_run),
                "--source-url",
                self.source_url,
                "--profile-mode",
                PROFILE_MODE,
                "--source-mode",
                SOURCE_MODE,
                "--skip-optimize",
                "--evaluation-mode",
                evaluation_mode,
                "--hold-seconds",
                str(int(hold_seconds)),
                "--targets-c",
                ",".join(str(target) for target in targets_c),
                "--output-dir",
                cli_arg_path(attempt_dir),
            ]
            if cooldown_temp_c is not None:
                cmd.extend(["--cooldown-temp-c", f"{float(cooldown_temp_c):.1f}"])
            if stage_timeout_seconds is not None:
                cmd.extend(["--stage-timeout-seconds", str(int(stage_timeout_seconds))])
            if warmup_timeout_seconds is not None:
                cmd.extend(["--warmup-timeout-seconds", str(int(warmup_timeout_seconds))])
            if cooldown_timeout_seconds is not None:
                cmd.extend(["--cooldown-timeout-seconds", str(int(cooldown_timeout_seconds))])
            if dry_run:
                cmd.extend(["--dry-run", "--device", "mock-fp-lab-01"])
            else:
                cmd.extend(["--device", self.resolve_device_id(False)])
            if seed_profile_file is not None:
                cmd.extend(["--seed-profile-file", cli_arg_path(seed_profile_file)])
            for candidate in candidate_profile_files or []:
                cmd.extend(["--candidate-profile-file", cli_arg_path(candidate)])
            output = self.run_json_command(cmd, retry_with_source_recovery=not dry_run)
            if output.get("kind") == "thermal_self_test_batch":
                batch_id = output["batchId"]
                summary_path = attempt_dir / batch_id / "batch.json"
                summary = read_json(summary_path)
                samples_path = summary.get("files", {}).get("samplesPath")
                last_result = SelfTestRun(
                    output=output,
                    summary=summary,
                    summary_path=summary_path,
                    run_dir=summary_path.parent,
                    samples_path=Path(samples_path) if isinstance(samples_path, str) else summary_path.parent,
                )
            else:
                summary_path = Path(output["files"]["summaryPath"])
                summary = read_json(summary_path)
                last_result = SelfTestRun(
                    output=output,
                    summary=summary,
                    summary_path=summary_path,
                    run_dir=Path(output["files"]["runDir"]),
                    samples_path=Path(output["files"]["samplesPath"]),
                )
            alarm_summary = self._run_alarm_summary(last_result)
            alarm_affected = bool(alarm_summary["faultReasons"]) or alarm_summary["faultAttentionPending"]
            self._record_alarm_attempt(last_result if alarm_affected else None)
            error_text = str(last_result.summary.get("error") or "")
            no_applied = not (last_result.summary.get("applied") or [])
            no_runs = not (last_result.summary.get("runs") or [])
            if (
                attempt_index == 0
                and "heater runtime readback enable mismatch" in error_text
                and (no_applied or no_runs)
            ):
                if alarm_affected or alarm_summary["faultAttentionPending"]:
                    self.clear_transient_temperature_warning()
                log(f"thermal self-test retrying after runtime readback mismatch: {error_text}")
                continue
            if attempt_index == 0 and self._run_has_transient_temperature_fault(last_result):
                self.clear_transient_temperature_warning()
                log(
                    "thermal self-test retrying after transient temperature warning:"
                    f" {self._run_fault_reason(last_result) or 'unknown'}"
                )
                continue
            if alarm_summary["faultAttentionPending"]:
                self.clear_transient_temperature_warning()
            return last_result
        if last_result is not None:
            return last_result
        raise RuntimeError("thermal self-test produced no result")

    def retune(self, run_dir: Path, target_temp_c: int) -> tuple[dict[str, Any], Path]:
        cmd = [
            str(self.flux_purr_bin),
            "--devd",
            self.devd_url,
            "--json",
            "thermal",
            "retune",
            "--run-dir",
            cli_arg_path(run_dir),
            "--optimize-targets-c",
            str(target_temp_c),
        ]
        self.run_json_command(cmd)
        candidate_path = run_dir / "thermal-profile.replayed.candidate.json"
        if not candidate_path.exists():
            raise RuntimeError(f"retune did not produce {candidate_path}")
        return read_json(candidate_path), candidate_path

    def disarm_and_clear_preview(self) -> None:
        if self.dry_run:
            return
        self.run_json_command(
            [
                str(self.flux_purr_bin),
                "--devd",
                self.devd_url,
                "--json",
                "runtime",
                "set",
                "--device",
                self.resolve_device_id(False),
                "--heater-enabled",
                "false",
                "--active-cooling",
                "true",
            ]
        )
        self.run_json_command(
            [
                str(self.flux_purr_bin),
                "--devd",
                self.devd_url,
                "--json",
                "thermal",
                "profile",
                "clear-preview",
                "--device",
                self.resolve_device_id(False),
            ]
        )

    def status(self) -> dict[str, Any]:
        if self.dry_run:
            return {}
        return self.run_json_command(
            [
                str(self.flux_purr_bin),
                "--devd",
                self.devd_url,
                "--json",
                "status",
                "--device",
                self.resolve_device_id(False),
            ]
        )

    def clear_transient_temperature_warning(self) -> bool:
        if self.dry_run:
            return False
        try:
            self.run_json_command(
                [
                    str(self.flux_purr_bin),
                    "--devd",
                    self.devd_url,
                    "--json",
                    "runtime",
                    "set",
                    "--device",
                    self.resolve_device_id(False),
                    "--heater-enabled",
                    "false",
                    "--active-cooling",
                    "true",
                ]
            )
        except RuntimeError:
            pass
        last_status: dict[str, Any] | None = None
        for _ in range(TRANSIENT_WARNING_CLEAR_POLL_ATTEMPTS):
            try:
                status = self.status()
            except RuntimeError:
                time.sleep(TRANSIENT_WARNING_CLEAR_POLL_INTERVAL_SECONDS)
                continue
            last_status = status
            fault_reason = status.get("heaterFaultReason")
            mode = status.get("mode")
            fault_attention_pending = bool(
                status.get("faultAttentionPending") or status.get("fault_attention_pending")
            )
            if fault_reason in TRANSIENT_TEMPERATURE_FAULTS:
                time.sleep(TRANSIENT_WARNING_CLEAR_POLL_INTERVAL_SECONDS)
                continue
            if fault_reason in (None, "") and mode != "fault":
                if fault_attention_pending:
                    try:
                        self.acknowledge_fault_attention()
                    except RuntimeError:
                        pass
                try:
                    self.run_json_command(
                        [
                            str(self.flux_purr_bin),
                            "--devd",
                            self.devd_url,
                            "--json",
                            "thermal",
                            "profile",
                            "clear-preview",
                            "--device",
                            self.resolve_device_id(False),
                        ]
                    )
                except RuntimeError:
                    pass
                return True
            break
        if last_status is not None:
            log(
                "transient temperature warning did not clear:"
                f" mode={last_status.get('mode')} heaterFaultReason={last_status.get('heaterFaultReason')}"
            )
        return False

    def _run_samples_path(self, run: SelfTestRun) -> Path | None:
        if run.samples_path.exists() and run.samples_path.is_file():
            return run.samples_path
        files = run.summary.get("files") if isinstance(run.summary.get("files"), dict) else {}
        samples_path = files.get("samplesPath")
        if isinstance(samples_path, str):
            candidate = Path(samples_path)
            if candidate.exists() and candidate.is_file():
                return candidate
        return None

    def _run_sample_records(self, run: SelfTestRun) -> list[dict[str, Any]]:
        samples_path = self._run_samples_path(run)
        if samples_path is None:
            return []
        records: list[dict[str, Any]] = []
        with samples_path.open("r", encoding="utf-8") as handle:
            for line in handle:
                if not line.strip():
                    continue
                try:
                    sample = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(sample, dict):
                    records.append(sample)
        return records

    def _sample_fault_reasons(self, sample: dict[str, Any]) -> list[str]:
        reasons: list[str] = []
        status = sample.get("status") if isinstance(sample.get("status"), dict) else {}
        heater = sample.get("heaterTelemetry") if isinstance(sample.get("heaterTelemetry"), dict) else {}
        for source in (status, heater):
            fault_reason = source.get("heaterFaultReason")
            if isinstance(fault_reason, str) and fault_reason and fault_reason not in reasons:
                reasons.append(fault_reason)
        return reasons

    def _sample_fault_attention_pending(self, sample: dict[str, Any]) -> bool:
        status = sample.get("status") if isinstance(sample.get("status"), dict) else {}
        return bool(status.get("faultAttentionPending") or status.get("fault_attention_pending"))

    def _run_alarm_summary(self, run: SelfTestRun) -> dict[str, Any]:
        samples = self._run_sample_records(run)
        fault_reasons: list[str] = []
        fault_attention_pending = False
        for sample in samples:
            for reason in self._sample_fault_reasons(sample):
                if reason not in fault_reasons:
                    fault_reasons.append(reason)
            fault_attention_pending = fault_attention_pending or self._sample_fault_attention_pending(sample)
        validation = run.summary.get("validation") if isinstance(run.summary.get("validation"), dict) else {}
        for failure in validation.get("failures") or []:
            if not isinstance(failure, dict):
                continue
            reason = failure.get("reason") or failure.get("failureReason") or failure.get("stopReason")
            if isinstance(reason, str) and reason in TRANSIENT_TEMPERATURE_FAULTS and reason not in fault_reasons:
                fault_reasons.append(reason)
        return {
            "runId": str(run.summary.get("runId") or ""),
            "summaryPath": str(run.summary_path),
            "samplesPath": str(self._run_samples_path(run) or run.samples_path),
            "targetTempsC": [
                int(stage.get("targetTempC"))
                for stage in (run.summary.get("applied") or [])
                if isinstance(stage, dict) and "targetTempC" in stage
            ],
            "faultReasons": fault_reasons,
            "faultAttentionPending": fault_attention_pending,
        }

    def _record_alarm_attempt(self, run: SelfTestRun | None) -> None:
        if run is None:
            self._consecutive_alarm_attempts.clear()
            return
        summary = self._run_alarm_summary(run)
        if not summary["faultReasons"] and not summary["faultAttentionPending"]:
            self._consecutive_alarm_attempts.clear()
            return
        self._consecutive_alarm_attempts.append(summary)
        if len(self._consecutive_alarm_attempts) >= 3:
            raise AlarmInterventionRequired(list(self._consecutive_alarm_attempts))

    def _run_fault_reason(self, run: SelfTestRun) -> str | None:
        if not run.samples_path.exists() or not run.samples_path.is_file():
            return None
        last_nonempty = ""
        with run.samples_path.open("r", encoding="utf-8") as handle:
            for line in handle:
                if line.strip():
                    last_nonempty = line
        if not last_nonempty:
            return None
        try:
            sample = json.loads(last_nonempty)
        except json.JSONDecodeError:
            return None
        status = sample.get("status")
        if isinstance(status, dict):
            fault_reason = status.get("heaterFaultReason")
            if isinstance(fault_reason, str) and fault_reason:
                return fault_reason
        heater = sample.get("heaterTelemetry")
        if isinstance(heater, dict):
            fault_reason = heater.get("heaterFaultReason")
            if isinstance(fault_reason, str) and fault_reason:
                return fault_reason
        return None

    def _run_has_transient_temperature_fault(self, run: SelfTestRun) -> bool:
        return self._run_fault_reason(run) in TRANSIENT_TEMPERATURE_FAULTS
