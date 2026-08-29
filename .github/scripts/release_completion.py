#!/usr/bin/env python3
"""Enforce the VERSION/release-completion contract for pull requests."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / ".github/scripts"))
import release_chain as CHAIN  # noqa: E402


class CompletionError(RuntimeError):
    pass


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True)
    if check and result.returncode != 0:
        raise CompletionError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout.strip()


def show_version(commit: str) -> str:
    try:
        text = subprocess.check_output(["git", "show", f"{commit}:VERSION"], cwd=ROOT, text=True)
    except subprocess.CalledProcessError as error:
        raise CompletionError(f"{commit} has no VERSION file") from error
    return CHAIN.PRODUCT_VERSION.read_version_from_text(text)


def has_version(commit: str) -> bool:
    return (
        subprocess.run(
            ["git", "cat-file", "-e", f"{commit}:VERSION"],
            cwd=ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        ).returncode
        == 0
    )


def local_tag_commit(tag: str) -> str | None:
    result = subprocess.run(
        ["git", "rev-list", "-n", "1", tag], cwd=ROOT, text=True, capture_output=True
    )
    return result.stdout.strip() if result.returncode == 0 else None


def gh_json(repository: str, path: str) -> object:
    result = subprocess.run(
        ["gh", "api", f"repos/{repository}/{path}"], cwd=ROOT, text=True, capture_output=True
    )
    if result.returncode != 0:
        raise CompletionError(result.stderr.strip() or f"gh api failed for {path}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise CompletionError(f"GitHub API returned invalid JSON for {path}") from error


def verify_remote_release(repository: str, release: dict[str, str]) -> None:
    tag = release["tag"]
    ref = gh_json(repository, f"git/ref/tags/{tag}")
    if not isinstance(ref, dict) or not isinstance(ref.get("object"), dict):
        raise CompletionError(f"GitHub tag {tag} response is invalid")
    tag_object = ref["object"]
    remote_sha = str(tag_object.get("sha", ""))
    if tag_object.get("type") == "tag":
        tag_payload = gh_json(repository, f"git/tags/{remote_sha}")
        if not isinstance(tag_payload, dict) or not isinstance(tag_payload.get("object"), dict):
            raise CompletionError(f"GitHub annotated tag {tag} response is invalid")
        remote_sha = str(tag_payload["object"].get("sha", ""))
    if remote_sha != release["releaseSha"]:
        raise CompletionError(f"GitHub tag {tag} points to {remote_sha}, expected {release['releaseSha']}")

    payload = gh_json(repository, f"releases/tags/{tag}")
    if not isinstance(payload, dict) or payload.get("draft") or not payload.get("published_at"):
        raise CompletionError(f"GitHub release {tag} is not published")
    assets = payload.get("assets")
    names = {asset.get("name") for asset in assets if isinstance(asset, dict)} if isinstance(assets, list) else set()
    required = {f"flux-purr-release-manifest-{tag}.json"}
    if not any(name.startswith(f"flux-purr-web-v{release['version']}") for name in names if isinstance(name, str)):
        raise CompletionError(f"GitHub release {tag} is missing the Web asset")
    if not any(name.startswith(f"flux-purr-firmware-v{release['version']}") for name in names if isinstance(name, str)):
        raise CompletionError(f"GitHub release {tag} is missing the firmware asset")
    if not any(
        name.startswith("flux-purr-host-tools-") and f"v{release['version']}" in name
        for name in names
        if isinstance(name, str)
    ):
        raise CompletionError(f"GitHub release {tag} is missing the host-tools asset")
    if not required.issubset(names):
        raise CompletionError(f"GitHub release {tag} is missing its release manifest")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", default="HEAD", help="pull request head commit")
    parser.add_argument("--base", default="origin/main", help="pull request base commit")
    parser.add_argument("--allow-migration", action="store_true")
    parser.add_argument("--migration-version", default="0.22.0")
    parser.add_argument("--repository", help="owner/name for remote release verification")
    parser.add_argument("--skip-remote", action="store_true")
    args = parser.parse_args(argv)
    try:
        commit = git("rev-parse", f"{args.commit}^{{commit}}")
        base = git("rev-parse", f"{args.base}^{{commit}}")
        base_has_version = has_version(base)
        if not base_has_version:
            if not args.allow_migration:
                raise CompletionError("base branch has no VERSION; migration permission is required")
            version = show_version(commit)
            if version != args.migration_version:
                raise CompletionError(
                    f"migration must establish VERSION={args.migration_version}, got {version}"
                )
            print(f"release completion: migration baseline {args.migration_version} allowed")
            return 0

        changed = git("diff", "--name-only", f"{base}...{commit}").splitlines()
        if "VERSION" in changed:
            raise CompletionError("ordinary pull requests must not modify VERSION")

        release = CHAIN.verify_release_commit(base)
        local_tag = local_tag_commit(release["tag"])
        if local_tag and local_tag != release["releaseSha"]:
            raise CompletionError(f"local tag {release['tag']} points to {local_tag}, expected {release['releaseSha']}")
        if not args.skip_remote:
            if not args.repository:
                raise CompletionError("--repository is required for remote release verification")
            verify_remote_release(args.repository, release)
        print(f"release completion: {release['tag']} at {release['releaseSha']}")
        return 0
    except (CompletionError, CHAIN.ReleaseChainError, CHAIN.PRODUCT_VERSION.VersionError) as error:
        print(f"release_completion.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
