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
- runtime status 现在显式回显 `faultAttentionPending`；runtime config、CLI `runtime set` 与 app live runtime 都支持 `faultAttentionAcknowledged=true`。attention 只属于热失控：`temp >= 420°C` 时每 `1s` 播放一次热失控提示，温度回落且未确认时退化为每 `10s` reminder。`SensorShort / SensorOpen / AdcReadFailed` 只停热并报告测温 fault，不蜂鸣、不进入 pending。owner-facing 温度显示在 RTD fault 期间保留最后一个有效读数，不再把 `0°C` 当作当前温度上报。
- `scripts/thermal_tuning_sprint.py` 当前默认以 `100w / pps5a` 的 `60 / 140 / 220°C` flagship 目标集执行预算化调优。每个目标从冷却等待开始计入 `20min` 预算，默认不再额外采集 `0% / 25% / 50%` approach-only 曲线；full-speed-to-stable gate 按目标温度动态选择：`targetTempC <= 150°C` 使用 `10s` 并要求至少 `1s` 确认裕量，`targetTempC > 150°C` 使用 `5s` 并要求至少 `0.5s` 确认裕量。候选生成按“未进下界、未进上界、进带后下破、进带后上破、通过但裕量不足”分类，只修改解释该类证据的 target-local 字段。preliminary review bundle 继续使用 canonical `index.html + run.bundle.json + samples.ndjson + thermal-profile.accepted.json` 格式，并按实际执行顺序保存所有有效 scout、batch candidate、confirm 与 recovery attempt，在目标 tab 中展示候选名、采用状态、失败分类、参数与温度/控制/source 图表。
- thermal tuning runner 在明确的 `SensorShort / SensorOpen / AdcReadFailed` 后只等待测温恢复并重试当前子步骤，不再把测温 fault 当作 attention reminder。若 runtime 报告真正的热失控 `faultAttentionPending`，runner 才发送 acknowledge；连续三次有效测试出现测温 fault 或热失控证据时，runner 会抛出 `thermal_alarm_pause` 并写出 `alarm-pause.json`，要求人工检查后重跑受影响测试。
- 当前 5A flagship bench fixture 固定为 Flux Purr 授权串口 `/dev/cu.usbmodem2111401` 与 IsolaPurr source `f293cc9c139e`（用户标识 `f293cc`）/ `http://192.168.31.224`。当前 repo-local sprint preflight 必须先确认 source readback 仍为 `100W`、PD enabled、PPS enabled、`pd_pps_5a=true`、`pps3_limit_ma=5000`、`tps_mode=auto_follow`，并且只允许通过同一 source 的 `USB-C disconnected -> auto` 输出切换做恢复；授权串口缺失、变号或 source identity 变化时停止，不自动切换。
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
- `thermal-self-test-runs/preliminary-pd100w-pps5a-60-140-220-20260717/` 已作为当前 5A preliminary review bundle：顶层回显 `bundleDisposition=preliminary_review`、`acceptedProfileRole=review_candidate_snapshot`、`selectedMode=100w`、`resolvedBank=pps5a`、`detectedSourceClass=pps5a`，并为 `60 / 140 / 220°C` 三个 tab 分别附带 `holdCheck`。当前三点单点 `60s` confirm 指标分别为：`60°C => overshoot 0.75 / p2p 1.71`、`140°C => overshoot 1.73 / p2p 2.94`、`220°C => overshoot 1.11 / p2p 1.59`。
- 当前 100W 剩余工作是用动态 full-speed-to-stable gate 收口 `60 / 140 / 220°C` flagship preliminary review，然后再决定是否扩展到完整 accepted baseline；历史 `0% / 25% / 50%` approach-only 曲线若要引用，只应作为显式诊断背景而不是当前默认判据。

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
