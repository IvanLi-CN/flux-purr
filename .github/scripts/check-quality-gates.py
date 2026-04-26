#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any


def fail(message: str) -> int:
    print(f"check-quality-gates.py: {message}", file=sys.stderr)
    return 1


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise SystemExit(fail(f"missing file: {path}"))


def workflow_name(text: str) -> str:
    match = re.search(r"(?m)^name:\s*(.+?)\s*$", text)
    return match.group(1).strip().strip('"\'') if match else ""


def workflow_job_names(text: str) -> set[str]:
    names = set(re.findall(r"(?m)^\s{4}name:\s*(.+?)\s*$", text))
    return {name.strip().strip('"\'') for name in names}


def require(condition: bool, message: str, errors: list[str]) -> None:
    if not condition:
        errors.append(message)


def validate_workflow(entry: dict[str, Any], repo_root: Path, errors: list[str]) -> None:
    path = repo_root / entry["path"]
    text = read_text(path)
    actual_name = workflow_name(text)
    require(actual_name == entry["workflow"], f"{entry['path']} workflow name is {actual_name!r}, expected {entry['workflow']!r}", errors)
    actual_jobs = workflow_job_names(text)
    for job_name in entry["jobs"]:
        require(job_name in actual_jobs, f"{entry['path']} missing job name {job_name!r}; found {sorted(actual_jobs)}", errors)


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    declaration_path = repo_root / ".github/quality-gates.json"
    declaration = json.loads(read_text(declaration_path))
    errors: list[str] = []

    require(declaration.get("schema_version") == 1, "schema_version must be 1", errors)
    required_checks = declaration.get("required_checks")
    require(isinstance(required_checks, list) and required_checks, "required_checks must be a non-empty list", errors)
    require("main" in declaration["policy"]["branch_protection"]["protected_branches"], "main must be protected", errors)
    require(declaration["policy"]["branch_protection"]["require_pull_request"] is True, "main must require pull requests", errors)
    require(declaration["policy"]["branch_protection"]["disallow_direct_pushes"] is True, "direct pushes must be disallowed", errors)
    require(declaration["policy"]["branch_protection"]["disallow_force_pushes"] is True, "force pushes must be disallowed", errors)
    require(declaration["policy"]["branch_protection"]["disallow_branch_deletions"] is True, "branch deletions must be disallowed", errors)
    require(declaration["policy"]["require_signed_commits"] is True, "signed commits must be required", errors)

    workflow_jobs: set[str] = set()
    for key in ("expected_pr_workflows", "expected_main_workflows", "expected_release_workflows"):
        for entry in declaration.get(key, []):
            validate_workflow(entry, repo_root, errors)
            workflow_jobs.update(entry["jobs"])

    for check in required_checks:
        require(check in workflow_jobs, f"required check {check!r} is not declared by expected workflows", errors)

    if errors:
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Quality gates declaration is internally consistent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
