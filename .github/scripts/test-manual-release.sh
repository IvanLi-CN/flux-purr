#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

cat > "${tmp_dir}/tags.txt" <<'EOF'
v0.30.7 0000000000000000000000000000000000000001
v0.30.8 0000000000000000000000000000000000000003
v0.30.0-rc.1 0000000000000000000000000000000000000002
EOF
cat > "${tmp_dir}/tag-intents.txt" <<'EOF'
v0.30.7 patch
v0.30.8 patch
v0.30.0-rc.1 rc
EOF
cat > "${tmp_dir}/tag-baselines.txt" <<'EOF'
v0.30.7 0.30.6
v0.30.8 0.30.7
v0.30.0-rc.1 0.30.7
EOF
cat > "${tmp_dir}/tag-reservations.txt" <<'EOF'
v0.30.7 v0.30.7@0000000000000000000000000000000000000001
v0.30.8 v0.30.8@0000000000000000000000000000000000000003
v0.30.0-rc.1 v0.30.0-rc.1@0000000000000000000000000000000000000002
EOF

resolve() {
  python3 "${root_dir}/.github/scripts/manual_release.py" resolve --intent "$1" --baseline 0.30.7 --tags-file "${tmp_dir}/tags.txt" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])'
}

test "$(resolve patch)" = 0.30.8
test "$(resolve minor)" = 0.31.0
test "$(resolve major)" = 1.0.0
test "$(resolve rc)" = 0.30.8-rc.1

printf '%s\n' docs/spec.md README README.md CHANGELOG.txt docs/image.png > "${tmp_dir}/docs.paths"
printf '%s\n' docs/spec.md web/src/app.tsx > "${tmp_dir}/mixed.paths"
test "$(python3 "${root_dir}/.github/scripts/manual_release.py" classify --paths-file "${tmp_dir}/docs.paths" | python3 -c 'import json,sys; print(json.load(sys.stdin)["release_required"])')" = False
test "$(python3 "${root_dir}/.github/scripts/manual_release.py" classify --paths-file "${tmp_dir}/mixed.paths" | python3 -c 'import json,sys; print(json.load(sys.stdin)["release_required"])')" = True

set +e
python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent patch --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000004 \
  --tags-file "${tmp_dir}/tags.txt" > "${tmp_dir}/collision.out" 2>&1
status=$?
set -e
test "${status}" -ne 0
grep -F "already owned" "${tmp_dir}/collision.out"

set +e
python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent minor --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000001 \
  --tags-file "${tmp_dir}/tags.txt" > "${tmp_dir}/source-tag.out" 2>&1
status=$?
set -e
test "${status}" -ne 0
grep -F "already has release tag" "${tmp_dir}/source-tag.out"

: > "${tmp_dir}/empty-tags.txt"
set +e
python3 "${root_dir}/.github/scripts/manual_release.py" baseline \
  --tags-file "${tmp_dir}/empty-tags.txt" --fallback-version 0.30.8-rc.1 > "${tmp_dir}/rc-baseline.out" 2>&1
status=$?
set -e
test "${status}" -ne 0
grep -F "trusted baseline must be stable" "${tmp_dir}/rc-baseline.out"

python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent rc --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000002 \
  --tags-file "${tmp_dir}/tags.txt" --intent-file "${tmp_dir}/tag-intents.txt" \
  --baseline-file "${tmp_dir}/tag-baselines.txt" --reservation-file "${tmp_dir}/tag-reservations.txt" \
  --operation recover \
  | python3 -c 'import json,sys; value=json.load(sys.stdin); assert value["version"] == "0.30.0-rc.1"; assert value["baseline"] == "0.30.7"; assert value["reservation_status"] == "matching"'

set +e
python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent minor --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000003 \
  --tags-file "${tmp_dir}/tags.txt" --intent-file "${tmp_dir}/tag-intents.txt" \
  --baseline-file "${tmp_dir}/tag-baselines.txt" --reservation-file "${tmp_dir}/tag-reservations.txt" \
  --operation recover > "${tmp_dir}/intent-mismatch.out" 2>&1
status=$?
set -e
test "${status}" -ne 0
grep -F "does not match reserved release intent patch" "${tmp_dir}/intent-mismatch.out"

python3 - "${root_dir}" <<'PY'
import argparse
import importlib.util
import json
import tempfile
from pathlib import Path
import sys

root = Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("product_release_manifest", root / ".github/scripts/product_release_manifest.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with tempfile.TemporaryDirectory() as directory:
    repo = Path(directory)
    (repo / "VERSION").write_text("0.30.8\n", encoding="utf-8")
    assets = repo / "release-assets"
    assets.mkdir()
    asset_names = {
        "flux-purr-web-v0.30.8.tar.gz": b"web",
        "flux-purr-web-demo-v0.30.8.tar.gz": b"demo",
        "flux-purr-firmware-v0.30.8.fluxpurr-fw": b"firmware",
        "flux-purr-firmware-v0.30.8.tar.gz": b"firmware-tar",
        "flux-purr-host-tools-linux-x64-v0.30.8.tar.gz": b"host",
    }
    for name, content in asset_names.items():
        (assets / name).write_bytes(content)
    components = [
        {
            "id": "web",
            "version": "0.30.8",
            "channel": "stable",
            "protocolVersions": [],
            "assets": ["flux-purr-web-v0.30.8.tar.gz", "flux-purr-web-demo-v0.30.8.tar.gz"],
        },
        {
            "id": "firmware",
            "version": "0.30.8",
            "channel": "stable",
            "protocolVersions": [],
            "assets": ["flux-purr-firmware-v0.30.8.fluxpurr-fw", "flux-purr-firmware-v0.30.8.tar.gz"],
        },
        {
            "id": "host-tools",
            "version": "0.30.8",
            "channel": "stable",
            "protocolVersions": [],
            "assets": ["flux-purr-host-tools-linux-x64-v0.30.8.tar.gz"],
        },
    ]
    args = argparse.Namespace(
        repo_root=repo,
        version="0.30.8",
        tag="v0.30.8",
        source_sha="0" * 40,
        channel="stable",
        intent="patch",
        baseline="0.30.7",
        reservation_id="v0.30.8@" + "0" * 40,
        previous_manifest=None,
        component=[json.dumps(component) for component in components],
        asset_root=str(assets),
    )
    manifest = module.build_manifest(args)
    assert manifest["releaseIntent"] == "patch"
    assert manifest["trustedBaseline"] == "0.30.7"
    assert manifest["reservationId"] == "v0.30.8@" + "0" * 40
    (assets / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    verifier_spec = importlib.util.spec_from_file_location(
        "verify_release_assets", root / ".github/scripts/verify_release_assets.py"
    )
    assert verifier_spec and verifier_spec.loader
    verifier = importlib.util.module_from_spec(verifier_spec)
    verifier_spec.loader.exec_module(verifier)
    verifier.verify(
        argparse.Namespace(
            asset_root=str(assets),
            manifest="manifest.json",
            version="0.30.8",
            tag="v0.30.8",
            source_sha="0" * 40,
            intent="patch",
            baseline="0.30.7",
            reservation_id="v0.30.8@" + "0" * 40,
        )
    )
    incomplete_manifest = dict(manifest, components=manifest["components"][:-1])
    (assets / "incomplete.json").write_text(json.dumps(incomplete_manifest), encoding="utf-8")
    try:
        verifier.verify(
            argparse.Namespace(
                asset_root=str(assets),
                manifest="incomplete.json",
                version="0.30.8",
                tag="v0.30.8",
                source_sha="0" * 40,
                intent="patch",
                baseline="0.30.7",
                reservation_id="v0.30.8@" + "0" * 40,
            )
        )
    except verifier.AssetVerificationError:
        pass
    else:
        raise AssertionError("incomplete component set must fail verification")
    tampered_manifest = json.loads(json.dumps(manifest))
    tampered_manifest["components"][0]["assets"][0]["channel"] = "rc"
    (assets / "tampered.json").write_text(json.dumps(tampered_manifest), encoding="utf-8")
    try:
        verifier.verify(
            argparse.Namespace(
                asset_root=str(assets),
                manifest="tampered.json",
                version="0.30.8",
                tag="v0.30.8",
                source_sha="0" * 40,
                intent="patch",
                baseline="0.30.7",
                reservation_id="v0.30.8@" + "0" * 40,
            )
        )
    except verifier.AssetVerificationError:
        pass
    else:
        raise AssertionError("tampered component metadata must fail verification")
    reconcile_spec = importlib.util.spec_from_file_location(
        "reconcile_release_assets", root / ".github/scripts/reconcile_release_assets.py"
    )
    assert reconcile_spec and reconcile_spec.loader
    reconcile = importlib.util.module_from_spec(reconcile_spec)
    reconcile_spec.loader.exec_module(reconcile)
    existing = repo / "existing-release"
    existing.mkdir()
    (existing / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    missing_name = "flux-purr-host-tools-linux-x64-v0.30.8.tar.gz"
    for path in assets.iterdir():
        if path.name not in {"manifest.json", missing_name}:
            (existing / path.name).write_bytes(path.read_bytes())
    upload_list = repo / "upload-list.txt"
    reconcile.reconcile(
        argparse.Namespace(
            existing_root=str(existing),
            built_root=str(assets),
            manifest="manifest.json",
            version="0.30.8",
            tag="v0.30.8",
            source_sha="0" * 40,
            intent="patch",
            baseline="0.30.7",
            reservation_id="v0.30.8@" + "0" * 40,
            upload_list=str(upload_list),
        )
    )
    assert upload_list.read_text(encoding="utf-8").splitlines() == ["release-assets/flux-purr-host-tools-linux-x64-v0.30.8.tar.gz"]
    try:
        bad = argparse.Namespace(**{**vars(args), "tag": "v9.9.9"})
        module.build_manifest(bad)
    except module.ManifestError:
        pass
    else:
        raise AssertionError("tag/version mismatch must fail")
    try:
        bad = argparse.Namespace(**{**vars(args), "baseline": "0.30.7-rc.1"})
        module.build_manifest(bad)
    except module.ManifestError:
        pass
    else:
        raise AssertionError("RC trusted baseline must fail")

archive_spec = importlib.util.spec_from_file_location(
    "create_release_archive", root / ".github/scripts/create_release_archive.py"
)
assert archive_spec and archive_spec.loader
archive = importlib.util.module_from_spec(archive_spec)
archive_spec.loader.exec_module(archive)
with tempfile.TemporaryDirectory() as archive_directory:
    archive_root = Path(archive_directory)
    first = archive_root / "first"
    second = archive_root / "second"
    for directory in (first, second):
        (directory / "nested").mkdir(parents=True)
        (directory / "nested" / "file.txt").write_text("same\n", encoding="utf-8")
        (directory / "root.txt").write_text("same\n", encoding="utf-8")
    first_archive = archive_root / "first.tar.gz"
    second_archive = archive_root / "second.tar.gz"
    archive.create_archive(first, first_archive, [])
    archive.create_archive(second, second_archive, [])
    assert first_archive.read_bytes() == second_archive.read_bytes()
PY

echo "manual release policy tests passed"
