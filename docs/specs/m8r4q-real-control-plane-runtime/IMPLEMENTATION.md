# Flux Purr 真实控制平面运行时实现状态（#m8r4q）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、当前真相与剩余缺口，不保留会话级流水账。

## Current Status

- Implementation: Web + browser Web Serial + `devd` + CLI + USB JSONL runtime loop 已覆盖 identity、network、status、runtime mutation、artifact verify、flash dry-run、real flash 与 monitor event 的真实传输路径
- Lifecycle: active
- Catalog note: 控制平面当前以 Web / native `devd` / CLI 为主，direct firmware HTTP 仍未实现

## Coverage / rollout summary

- thermal profile persistence 已升级为固定 `pps3a` / `pps5a` 双 bank；每个 bank 最多持久化 `6` 个 populated anchors。EEPROM v2 使用固定 `2 KiB` slot，读取顺序为 `2 KiB v2 -> 1 KiB v1 -> 512 B legacy`，旧单 profile 会迁移到 `pps3a` 且默认 mode 为 `65w`。
- runtime status、runtime config、CLI 与 self-test 已统一支持 `thermalProfileMode=auto|65w|100w` 与 `thermalProfileResolvedBank`。显式 `65w` / `100w` 为强制档；`auto` 仅按 source capability class 在 `pps3a` / `pps5a` 间解析，不按 live current 自动回退。
- `flux-purr thermal profile preview|save|clear-saved` 已是 bank-aware 路径。`preview` 仍是单一 RAM overlay；显式 `save` / `clear-saved` 会携带目标 bank，`auto` 下会先读取 runtime status 再解析当前 resolved bank。
- `flux-purr thermal self-test` 已支持 source-aware `auto|65w|100w`。65W 维持 `20V / 3.25A` 语义，100W 使用 `21V / 5A` 语义。报告与 HTML 已保留 `selectedMode`、`resolvedBank`、`detectedSourceClass`、source preset/readback 以及 per-stage `analysis.approachSource` / `analysis.holdSource`。
- `scripts/thermal_approach_characterization.py` 已支持 `100w / pps5a` 的 per-target dual-approach characterization：同一目标温度同时采集 `0加热` 与 `50%最低功率` 的 approach-only 曲线，并输出 canonical HTML bundle。当前冻结总报告位于 `thermal-self-test-runs/approach-characterization-pd100w-pps5a-20260717-final/`，覆盖 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C` 九个目标温度。
- approach characterization 的 brake 搜索当前真相是：`timeout_without_valid_rollback` 在未进入目标带时、以及 `never_entered_approach`，都必须继续归类为 `more_heat`。否则高温点会错误回跳到更大的 brake，浪费真机轮次。
- `pps3a` 默认 seed 来自 committed 65W accepted bundle。`pps5a` 在 committed accepted bundle 缺失时回退到 repo-local tuning seed `thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json`。当前 100W 路径已具备 end-to-end bank、source metadata、preview/save/retune 语义，但 `pps5a` accepted EEPROM save 与 frozen baseline 仍未收口。
- thermal self-test 默认调优阶梯为 `60 / 140 / 220°C`；支持的完整 ladder 为 `60 / 100 / 140 / 180 / 220 / 250°C`。`300°C` 不属于首版验收。默认 host sampling interval 为 `300ms`，持续采样下限为 `3Hz`；accepted comparison bundle 可以使用更高采样率。
- 当前固件与 host 工具链已统一 warmup 语义：只要 heater state machine 仍在 `warmup`，输出就保持 `100%`，host readback / candidate import / replay / report 都必须把 `warmupPowerPermille=1000` 视为唯一有效运行值。
- heater 控制环当前为 `20Hz`；每个 control cycle 聚合 `64` 次 RTD ADC conversion，并丢弃前置 settle 样本后保留分数毫伏均值贯穿 calibration 与 PT1000 转换。默认 `tempFilterAlphaPermille=750`，仍可通过 thermal profile API / EEPROM 覆盖。冻结的 `pps3a` accepted bundle 仍记录 `700`，因此历史 3A bundle 与当前固件默认值应分开理解。
- 当前温度链路已经收口为两条职责分离的路径：owner-facing `currentTempC` / `boardTempCenti` / front panel 直接反映当前有效 RTD 样本；controller EMA 与 slope 继续作为内部控制状态单独暴露。PPS transition guard 只能作用于 controller 内部状态，不得冻结 owner-facing 温度。
- 控制环前不得再叠加任何多样本窗口、中位数、输出钳位或速率限制。当前实现只保留“当前 RTD 样本 -> EMA”这一条控制温度路径，避免把加热和降温速度人为钝化。
- warmup handoff 现在要求实际温度步进确认，避免单个 RTD 批次跳变在滤波温度仍落后时提前退出 warmup。low-temp `Approach -> Hold` 零输出 seam 也已修正：只有当实际误差已进入 hold 释放带时才允许 predictive coast 维持零输出。
- host-side retune 已把 `warmupExitedAtMs -> firstHoldAtMs` 区间的 ideal Approach 曲线偏差纳入当前真相：当前 fit basis 固定为 `target_error_from_approach_start`，即用 `approachStartTempC -> targetTempC` 的归一化 target-error 曲线做 first-pass 分类，再决定是 `brake_late_or_residual`、`underpowered_or_early_coast` 还是 `oscillatory_near_target`。ambient 目前不是硬依赖；在样本未提供稳定 ambient 字段时，retune 不得阻塞，而是必须显式记录这一 fit basis。完成曲线分类后，retune 才继续区分 low-temp bounded residual 与 hold-entry carry。bounded residual 只做轻量 brake / cutoff / off 微调；只有明显 hold ripple 且 hold 输出高于基线时，才允许直接削减 hold sustain / reheat。
- `heaterCurrentReserveMa` 已进入 thermal profile settings、status 回显、preview/save API 与 EEPROM。heater safe-max 会在 source current capability 之上预留 reserve，而不是吃满整条 source 电流预算。
- `devd` 提供 localhost daemon、授权端口 serial discovery、lease、bounded events、USB identity/network/status/WiFi/runtime bridge、artifact verify、dry-run 与 real flash command boundary。真实烧录路径固定为 repo-local `flux-purr -> devd -> espflash`，并继续受授权端口纪律保护。
- `devd` 与相关 smoke 路径已固定几条 transport guardrail：显式 bind / serial / artifact root；授权串口缺失时拒绝自动切换到重新枚举端口；real flash 前释放 daemon-local serial session；浏览器与脚本通过 lease 复用同一设备会话而不是重复抢占串口。
- 当前控制平面已经具备 mock HTTP contract smoke、CLI-through-devd smoke、browser Web-to-devd smoke、runtime mutation/readback、artifact verify、flash dry-run、real flash、WiFi redaction 与 calibration/dashboard 关键路径的自动化或脚本化覆盖。

## Thermal acceptance state

- `pps3a` / 65W 稀疏 acceptance bundle 已存在，并继续作为 3A 当前基准。
- `pps5a` / 100W 路径已经具备 source-class 识别、bank 解析、preview seed、retune、approach characterization 与报告链路；`thermal-self-test-runs/approach-characterization-pd100w-pps5a-20260717-final/` 已作为当前 5A approach-reference 当前真相。
- 当前 100W 剩余工作不在 characterization 采集本身，而在后续把这些双曲线门槛进一步 materialize 到 retune / acceptance 判定与最终 accepted baseline。

## Remaining Gaps

- `pps5a` accepted EEPROM save 与 frozen baseline bundle 仍未完成；100W 路径虽已有 approach characterization current truth，但尚未形成 committed accepted bundle。
- direct firmware HTTP / `net_http` server 仍未实现；当前真实硬件 runtime 控制路径仍以 browser Web Serial 或 Web / CLI -> `devd` -> USB JSONL 为准。
- 完整 artifact catalog 管理页不属于本 spec 范围。
- macOS 打开 ESP32-S3 USB Serial/JTAG port 仍可能触发一次设备 reset；`devd` 的稳定性契约仍是避免 Web / daemon polling 期间反复 open / close 造成持续重启。

## References

- `./SPEC.md`
- `../../solutions/device-control/thermal-control-self-test.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
