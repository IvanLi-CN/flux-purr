"""Exercise CI install recovery without installing or removing real toolchains."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/actions/setup-xtensa/install.sh"
FAKE_ESPUP = '''#!/usr/bin/env python3
import os
from pathlib import Path
import sys

root = Path(os.environ["FIXTURE_DIR"])
args = sys.argv[1:]
with (root / "calls").open("a") as log:
    log.write(" ".join(args) + "\\n")
mode = os.environ["MODE"]
if args[0] == "uninstall":
    if mode == "cleanup_failure":
        sys.exit(7)
    (root / "partial").unlink(missing_ok=True)
    sys.exit(0)
count_file = root / "count"
count = int(count_file.read_text()) + 1 if count_file.exists() else 1
count_file.write_text(str(count))
if mode in ["success", "log_failure"] or (
    mode in ["recover", "cleanup_failure"] and count > 1
):
    if (root / "partial").exists():
        print("partial installation incorrectly reused")
        sys.exit(8)
    sys.exit(0)
(root / "partial").write_text("incomplete")
if mode == "invalid":
    print("Error: unsupported build target")
else:
    print("Error: HTTP GET Error: 504 Gateway Timeout")
sys.exit(9)
'''


class RecoveryTests(unittest.TestCase):
    def run_case(self, mode, ci=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            espup = root / "espup"
            espup.write_text(FAKE_ESPUP)
            espup.chmod(0o755)
            sleep = root / "sleep"
            sleep.write_text(
                '#!/bin/sh\nprintf "sleep %s\\n" "$1" >> "$FIXTURE_DIR/calls"\n'
            )
            sleep.chmod(0o755)
            if mode == "log_failure":
                tee = root / "tee"
                tee.write_text("#!/bin/sh\nexit 23\n")
                tee.chmod(0o755)
            environment = os.environ.copy()
            environment.update(
                PATH=str(root) + os.pathsep + environment["PATH"],
                FIXTURE_DIR=directory,
                MODE=mode,
                RUNNER_TEMP=directory,
                GITHUB_ACTIONS="true" if ci else "false",
            )
            result = subprocess.run(
                ["bash", str(SCRIPT), str(espup), "1.95.0.0", "esp32s3"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )
            log = root / "calls"
            calls = log.read_text().splitlines() if log.exists() else []
            return result, calls

    def test_success_no_retry(self):
        result, calls = self.run_case("success")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(calls), 1)

    def test_504_recovers_only_after_partial_install_cleanup(self):
        result, calls = self.run_case("recover")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(calls[1], "uninstall --name esp")
        self.assertEqual(calls[2], "sleep 15")
        self.assertEqual(len(calls), 4)

    def test_persistent_504_fails_after_three_attempts(self):
        result, calls = self.run_case("persistent")
        self.assertEqual(result.returncode, 9)
        self.assertEqual(sum(call.startswith("install") for call in calls), 3)
        self.assertEqual(
            [call for call in calls if call.startswith("sleep")],
            ["sleep 15", "sleep 45"],
        )

    def test_non_network_error_is_not_retried(self):
        result, calls = self.run_case("invalid")
        self.assertEqual(result.returncode, 9)
        self.assertEqual(len(calls), 1)

    def test_failed_cleanup_stops_before_retry(self):
        result, calls = self.run_case("cleanup_failure")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(sum(call.startswith("install") for call in calls), 1)
        self.assertEqual(len(calls), 2)

    def test_log_write_failure_is_not_success(self):
        result, calls = self.run_case("log_failure")
        self.assertEqual(result.returncode, 23)
        self.assertEqual(len(calls), 1)

    def test_refuses_non_ci_host(self):
        result, calls = self.run_case("success", ci=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
