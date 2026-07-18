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
    base_targets = [60, 140, 220]
    base_points: list[dict[str, Any]] = []
    for target_temp_c in base_targets:
        point = preliminary_points.get(target_temp_c) or fallback_points.get(target_temp_c)
        if point is None:
            raise RuntimeError(f"missing required base point {target_temp_c}°C for sparse seed")
        base_points.append(dict(point))

    point_240 = preliminary_points.get(240)
    if point_240 is None:
        materialized_240 = dry_run_materialized_points(
            runner,
            sparse_profile(settings, [fallback_points[220], fallback_points[250]]),
            [240],
            output_dir,
            "materialize-240",
        )
        point_240 = materialized_240.get(240)
    if point_240 is None:
        raise RuntimeError("unable to derive initial 240°C anchor")

    scaffold = sparse_profile(settings, [*base_points, dict(point_240)])
    derived = dry_run_materialized_points(runner, scaffold, [100, 180], output_dir, "materialize-100-180")
    points = [
        dict(base_points[0]),
        dict(derived[100]),
        dict(base_points[1]),
        dict(derived[180]),
        dict(base_points[2]),
        dict(point_240),
    ]
    by_target = {int(point["targetTempC"]): point for point in points}
    for target_temp_c in anchors_c:
        if target_temp_c not in by_target:
            raise RuntimeError(f"initial sparse seed missing anchor {target_temp_c}°C")
    return sparse_profile(settings, [by_target[target] for target in anchors_c])


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
    metrics = stage_metrics(scout_stage)
    evidence = stability_evidence_for_stage(scout_stage, scout_samples or [], target_temp_c)
    predicted_point = predict_next_point(current_point, evidence)

    variants = [CandidateVariant("current", current_profile)]
    if predicted_point != current_point:
        variants.append(CandidateVariant(str(evidence["failureClass"]), merge_point(current_profile, predicted_point)))

    hold_p2p = metrics.get("holdPeakToPeakC")
    if isinstance(hold_p2p, (int, float)) and float(hold_p2p) > 3.0:
        ripple_point = mutate_hold_ripple(current_point, metrics)
        if ripple_point != current_point:
            variants.append(CandidateVariant("hold_ripple", merge_point(current_profile, ripple_point)))

    if len(variants) == 1 and retuned_point != current_point:
        variants.append(CandidateVariant("retuned_fallback", retuned_profile))
    return variants[:4]
