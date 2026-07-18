from .core import *


@dataclass
class SelfTestRun:
    output: dict[str, Any]
    summary: dict[str, Any]
    summary_path: Path
    run_dir: Path
    samples_path: Path


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
        matches = [device for device in devices if isinstance(device, dict) and device.get("portPath") == self.authorized_port]
        if len(matches) != 1:
            raise RuntimeError(
                f"expected exactly one device on authorized port {self.authorized_port}, got {len(matches)}"
            )
        self._resolved_device_id = str(matches[0]["id"])
        return self._resolved_device_id

    def recover_source_output(self) -> None:
        if self.dry_run:
            return
        log("source recovery: restart IsolaPurr USB-C output on the same authorized source")
        self.run_subprocess_json(
            [
                "isolapurr",
                "power",
                "output",
                "manual",
                "--url",
                self.source_url,
                "--usb-c-path",
                "disconnected",
                "--json",
            ]
        )
        disconnect_error: RuntimeError | None = None
        for _ in range(SOURCE_RECOVERY_POLL_ATTEMPTS):
            disconnected = self.run_subprocess_json(
                ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
            )
            try:
                verify_isolapurr_power_show(disconnected, expect_usb_c_enabled=False)
                disconnect_error = None
                break
            except RuntimeError as exc:
                disconnect_error = exc
                time.sleep(SOURCE_RECOVERY_POLL_INTERVAL_SECONDS)
        if disconnect_error is not None:
            raise disconnect_error
        time.sleep(SOURCE_RECOVERY_SETTLE_SECONDS)
        self.run_subprocess_json(
            ["isolapurr", "power", "output", "auto", "--url", self.source_url, "--json"]
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
        attempts = 2 if retry_with_source_recovery and self.auto_recover_source and not self.dry_run else 1
        last_error: RuntimeError | None = None
        for attempt in range(attempts):
            log(f"$ {shlex.join(cmd)}")
            proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                if not proc.stdout.strip():
                    raise RuntimeError(f"command produced empty stdout: {shlex.join(cmd)}")
                return json.loads(proc.stdout)
            last_error = RuntimeError(
                f"command failed ({proc.returncode}): {shlex.join(cmd)}\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
            )
            if attempt + 1 < attempts:
                self.recover_source_output()
        raise last_error if last_error is not None else RuntimeError(f"command failed: {shlex.join(cmd)}")

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
                self.source_id,
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
                last_result = SelfTestRun(
                    output=output,
                    summary=summary,
                    summary_path=summary_path,
                    run_dir=summary_path.parent,
                    samples_path=summary_path.parent,
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
            error_text = str(last_result.summary.get("error") or "")
            no_applied = not (last_result.summary.get("applied") or [])
            no_runs = not (last_result.summary.get("runs") or [])
            if (
                attempt_index == 0
                and "heater runtime readback enable mismatch" in error_text
                and (no_applied or no_runs)
            ):
                log(f"thermal self-test retrying after runtime readback mismatch: {error_text}")
                continue
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
