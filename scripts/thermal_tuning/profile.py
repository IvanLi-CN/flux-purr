from .config import *
from .analysis import predict_next_point, stage_metrics, stability_evidence_for_stage


def profile_points(profile: dict[str, Any]) -> list[dict[str, Any]]:
    points = profile.get("points") or []
    return [dict(point) for point in points if isinstance(point, dict)]


def point_map(profile: dict[str, Any]) -> dict[int, dict[str, Any]]:
    return {
        int(point["targetTempC"]): dict(point)
        for point in profile_points(profile)
        if "targetTempC" in point
    }


def explicit_point(profile: dict[str, Any], target_temp_c: int) -> dict[str, Any] | None:
    return point_map(profile).get(int(target_temp_c))


def pad_profile_points(points: list[dict[str, Any]]) -> list[Any]:
    values: list[Any] = [dict(point) for point in sorted(points, key=lambda item: int(item["targetTempC"]))]
    while len(values) < THERMAL_CONTROL_PROFILE_MAX_POINTS:
        values.append(None)
    return values


def sparse_profile(settings: dict[str, Any], points: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "settings": dict(settings),
        "points": pad_profile_points(points),
    }


def repo_display_path(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def cli_arg_path(path: Path) -> str:
    if path.is_absolute():
        return repo_display_path(path)
    return str(path)


def merge_point(profile: dict[str, Any], point: dict[str, Any]) -> dict[str, Any]:
    target = int(point["targetTempC"])
    merged = point_map(profile)
    merged[target] = dict(point)
    return sparse_profile(dict(profile.get("settings") or {}), list(merged.values()))


def pick_profile_settings(*profiles: dict[str, Any]) -> dict[str, Any]:
    for profile in profiles:
        settings = profile.get("settings")
        if isinstance(settings, dict):
            return dict(settings)
    raise RuntimeError("no profile settings available")


def choose_first_sample_points(samples_path: Path) -> dict[int, dict[str, Any]]:
    points: dict[int, dict[str, Any]] = {}
    with samples_path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            sample = json.loads(line)
            target_temp_c = int(sample["targetTempC"])
            if target_temp_c not in points:
                heater_parameters = sample.get("heaterParameters")
                if isinstance(heater_parameters, dict):
                    points[target_temp_c] = dict(heater_parameters)
    return points


def dry_run_materialized_points(
    runner: "FluxPurrRunner",
    seed_profile: dict[str, Any],
    targets_c: list[int],
    output_dir: Path,
    tag: str,
) -> dict[int, dict[str, Any]]:
    seed_path = output_dir / f"{tag}.seed.json"
    write_json(seed_path, seed_profile)
    run = runner.self_test(
        seed_profile_file=seed_path,
        targets_c=targets_c,
        hold_seconds=12,
        output_dir=output_dir / f"{tag}-materialize",
        dry_run_override=True,
    )
    return choose_first_sample_points(run.samples_path)


def build_initial_sparse_seed(
    runner: "FluxPurrRunner",
    preliminary_profile: dict[str, Any],
    fallback_profile: dict[str, Any],
    anchors_c: list[int],
    output_dir: Path,
) -> dict[str, Any]:
    settings = pick_profile_settings(preliminary_profile, fallback_profile)
    preliminary_points = point_map(preliminary_profile)
    fallback_points = point_map(fallback_profile)
    requested_targets = [int(target) for target in anchors_c]
    if not requested_targets or len(set(requested_targets)) != len(requested_targets):
        raise RuntimeError("sparse seed targets must be a non-empty unique list")

    points: list[dict[str, Any]] = []
    for target_temp_c in requested_targets:
        point = preliminary_points.get(target_temp_c) or fallback_points.get(target_temp_c)
        if point is None:
            source_profile = preliminary_profile if preliminary_points else fallback_profile
            materialized = dry_run_materialized_points(
                runner,
                source_profile,
                [target_temp_c],
                output_dir,
                f"materialize-{target_temp_c}",
            )
            point = materialized.get(target_temp_c)
        if point is None:
            raise RuntimeError(f"initial sparse seed missing target {target_temp_c}°C")
        points.append(dict(point))
    return sparse_profile(settings, points)


def normalize_sparse_profile(
    runner: "FluxPurrRunner",
    profile: dict[str, Any],
    anchors_c: list[int],
    output_dir: Path,
    tag: str,
) -> dict[str, Any]:
    settings = pick_profile_settings(profile)
    explicit = point_map(profile)
    missing = [target for target in anchors_c if target not in explicit]
    materialized: dict[int, dict[str, Any]] = {}
    if missing:
        materialized = dry_run_materialized_points(runner, profile, missing, output_dir, tag)
    points: list[dict[str, Any]] = []
    for target_temp_c in anchors_c:
        point = explicit.get(target_temp_c) or materialized.get(target_temp_c)
        if point is None:
            raise RuntimeError(f"sparse normalization could not materialize {target_temp_c}°C")
        points.append(dict(point))
    return sparse_profile(settings, points)


def mutate_more_heat(point: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    mutated = dict(point)
    scale = 2 if target_temp_c >= 220 else 1 if target_temp_c <= 100 else 1.5
    # `holdEntryCentiC` is an error band below target. Lowering it delays Hold entry
    # until the controller is closer to target, which is the "more heat before Hold"
    # direction. Raising it would enter Hold earlier and worsen low-temperature
    # full-speed-to-stable failures.
    mutated["holdEntryCentiC"] = clamp_int(int(mutated["holdEntryCentiC"]) - int(25 * scale), 0, 5000)
    mutated["approachFloorPowerPermille"] = clamp_int(
        int(mutated["approachFloorPowerPermille"]) + int(30 * scale),
        0,
        1000,
    )
    mutated["holdPowerPermille"] = clamp_int(int(mutated["holdPowerPermille"]) + int(25 * scale), 0, 1000)
    mutated["holdReheatPowerPermille"] = clamp_int(
        int(mutated["holdReheatPowerPermille"]) + int(40 * scale),
        0,
        1000,
    )
    if target_temp_c >= 180:
        mutated["approachPowerPermille"] = clamp_int(int(mutated["approachPowerPermille"]) + 20, 0, 1000)
    return mutated


def mutate_more_brake(point: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    mutated = dict(point)
    brake = int(mutated["brakeDistanceCentiC"])
    mutated["brakeDistanceCentiC"] = clamp_int(brake + max(40, int(round(brake * 0.12))), 0, 5000)
    mutated["approachDampingExponentPermille"] = clamp_int(
        int(mutated["approachDampingExponentPermille"]) + (180 if target_temp_c <= 140 else 120),
        0,
        4000,
    )
    mutated["approachLeadTicks"] = clamp_int(int(mutated["approachLeadTicks"]) + 1, 0, 255)
    mutated["holdEntryCentiC"] = clamp_int(int(mutated["holdEntryCentiC"]) - (15 if target_temp_c <= 140 else 8), 0, 5000)
    mutated["holdReheatPowerPermille"] = clamp_int(int(mutated["holdReheatPowerPermille"]) - 30, 0, 1000)
    return mutated


def mutate_hold_ripple(point: dict[str, Any], metrics: dict[str, Any]) -> dict[str, Any]:
    mutated = dict(point)
    median = metrics.get("holdMedianOutputPermille")
    p90 = metrics.get("holdP90OutputPermille")
    if isinstance(median, (int, float)):
        hold_power = clamp_int(int(round(float(median))), 0, 1000)
        mutated["holdPowerPermille"] = max(hold_power, clamp_int(int(mutated["holdPowerPermille"]) - 20, 0, 1000))
    if isinstance(p90, (int, float)):
        mutated["holdReheatPowerPermille"] = clamp_int(
            max(int(round(float(p90))) + 20, int(mutated["holdPowerPermille"]) + 20),
            0,
            1000,
        )
    mutated["holdBlendTicks"] = clamp_int(int(mutated["holdBlendTicks"]) + 1, 1, 255)
    hold_on = int(mutated["holdOnCentiC"])
    hold_off = int(mutated["holdOffCentiC"])
    mutated["holdOffCentiC"] = clamp_int(max(hold_on + 20, hold_off - 10), 0, 5000)
    return mutated


@dataclass
class CandidateVariant:
    name: str
    profile: dict[str, Any]
    path: Path | None = None


def build_candidate_variants(
    current_profile: dict[str, Any],
    retuned_profile: dict[str, Any],
    target_temp_c: int,
    scout_stage: dict[str, Any],
    scout_samples: list[dict[str, Any]] | None = None,
) -> list[CandidateVariant]:
    current_point = explicit_point(current_profile, target_temp_c)
    retuned_point = explicit_point(retuned_profile, target_temp_c)
    if current_point is None or retuned_point is None:
        raise RuntimeError(f"missing {target_temp_c}°C anchor while building variants")
    evidence = stability_evidence_for_stage(scout_stage, scout_samples or [], target_temp_c)
    predicted_point = predict_next_point(current_point, evidence)

    variants = [CandidateVariant("current", current_profile)]
    if predicted_point != current_point:
        variants.append(CandidateVariant(str(evidence["failureClass"]), merge_point(current_profile, predicted_point)))
    return variants
