# Flux Purr 正式 PID 加热闭环与前面板运行态同步（#q2aw6）

## 状态

- Status: 已完成
- Created: 2026-04-21
- Last: 2026-04-25

## 背景 / 问题陈述

- 当前 `flux-purr` 已完成前面板输入、RTD 读取、CH224Q 默认电压请求与 heater/fan bring-up，但 Dashboard 的风扇语义仍残留旧的单布尔开关口径。
- `#223uj` 与 `#fk3u7` 已冻结前面板视觉和五向输入基线，但 Dashboard 的 fan line、Active Cooling 页面和过温告警仍缺少统一真相源。
- 若不把风扇策略、过温停热、feature-selected PD 默认请求与前面板显示一次收口，后续板级调试会持续混淆“策略开关”“实际输出”“保护联动”三套状态。

## 目标 / 非目标

### Goals

- 把 `GPIO47` 固定占空比加热替换为按 `target_temp_c` 驱动的正式闭环；当 CH224Q 读取到 PPS APDO 覆盖 `20V` 时，heater 后端使用受安全上限约束的 PPS/AVS 粗粒度调压与 `100Hz` MOS PWM 功率合成，否则回退同一 `GPIO47` PWM 调功路径。
- 加热闭环采用模型辅助 ramp/soak 与保温 PI 微调的混合控制器。控制器输出统一的等效热功率请求；PPS 后端映射为 `100mV` 对齐电压，PWM 负责连续的物理功率调节，固定 PD 后端选择不低于目标等效电压的 PDO 并使用同一 MOS PWM 合成等效功率。
- 支持 `ThermalControlProfile` preview 与显式保存。RAM preview 最多 10 个目标点；持久化保存固定为 `pps3a` / `pps5a` 双 bank，各最多 6 个非空已配置目标点并压紧稀疏槽位。持久化后端优先使用 EEPROM，EEPROM 不可达时使用 ESP flash fallback。`thermalProfileMode` 是 `auto|65w|100w`：auto 仅在 advertised PPS APDO 覆盖 `20V` 且 `ppsMaxMa >= 5000` 时解析 `pps5a`，显式档位不回退、不阻止运行；控制限压和保护继续按 live current 与 reserve。preview 始终优先于 selected bank。
- 提供 CLI/devd 自测试入口，抽象 bench source provider；当前默认且验收支持的 provider 是 IsolaPurr released CLI，用于准备 `auto|65w|100w`、PD Fixed enabled、PPS enabled、`auto_follow` 外部 source。单次 live run 工作目录只保留 `run.json`、`samples.ndjson` 与 `thermal-profile.candidate.json` 这类数据文件；owner-facing 冻结 baseline bundle 以浏览器可直接打开的 `index.html` 为唯一 canonical report，并同时提交 `run.bundle.json`、`samples.ndjson` 与 `thermal-profile.accepted.json`。每个 applied stage 的 `analysis` 必须同时沉淀 `approachSource` / `holdSource`，记录 source 实际电压、电流、功率在该窗口内的 `sampleCount`、`min/max/avg/first/last`。当 RTD 进入 fault 时，前面板与 runtime display 必须保留最后一个有效温度显示，不能把 `0°C` 伪装成当前温度；待确认告警由前面板输入或 runtime/CLI/app 的 `faultAttentionAcknowledged` 清除。
- 让 Dashboard 稳定显示实时温度、设定温度、`OFF/AUTO/RUN` 三态风扇显示与实际 heater 输出强度。
- 冻结正式风扇/保护包线：
  - heater `OFF` 且 active cooling `ON`：`40~60°C` 以 `GPIO36 duty=50%`（`500‰`）运行、`>60°C` 以 `GPIO36 duty=0%`（`0‰`）全速；一旦温度回落到 `<40°C`，继续以 `GPIO36 duty=100%`（`1000‰`）拖尾 `30s` 后再关闭。
  - heater `ON`：`<=100°C` 不主动散热；超过 `100°C` 后，只有实时 heater 输出大于 `0%` 时才进入最低电压 `0.2Hz` 使能脉冲，脉冲占空比为 cooling-disabled 脉冲的两倍并封顶 `50%`。
  - active cooling `OFF`：`>100°C` 进入最低电压 `0.2Hz` 使能脉冲，脉冲占空比按 `floor((temp-100)/10)%` 递增并封顶 `25%`。
  - active cooling `OFF` 且 `>350°C`：锁住停热并保持风扇 `50%`；`>360°C` 改为全速。
  - `temp >= 420°C`：进入热失控并保持 heater hard cutoff；在告警未确认期间禁止重新发起加热，并按现有主动降温包线强制风扇持续工作：`>60°C` 全速、`40~60°C` 为 `50%`。温度 `<40°C` 或收到告警确认时，两者任一先发生即解除该强制风扇状态。
- 默认启动时把 CH224Q 请求固定为 `20V`，再读取 CH224Q `0x60~0x8F` power data；只有 PPS capability 覆盖 `20V` 时才启用可调加热后端。自动加热时可调请求上限必须受 source capability 与 `I_source_max * R_estimated(T)` 的较小值限制；`R_estimated(T)` 使用当前 `3.2 ohm` heater load class 的一阶铜电阻估算。
- 产出 merge-ready 所需的 spec、视觉证据、板级验证与 review 收敛材料。

### Non-goals

- 不提供任意源码常量热调参入口；运行时调参必须通过 `ThermalControlProfile` 的持久化/API 可控字段完成。
- 不把 `>250°C` 纳入 thermal self-test 或首版调参验收。
- 不实现 fan tach 闭环、4 线 PWM、持久化风扇档位或按 VIN 自动切换固定 PD 请求。
- 不修改外部 HTTP / RPC / 持久化字段结构。
- 不扩展新的前面板菜单层级或联网业务逻辑。

## 范围（Scope）

### In scope

- `firmware/src/bin/flux_purr.rs`
- `firmware/src/frontpanel/**`
- `firmware/src/bin/frontpanel_preview.rs`
- `web/src/features/frontpanel-preview/**`
- `web/src/stories/FrontPanelDisplay.stories.tsx`
- `firmware/README.md`
- `docs/interfaces/http-api.md`
- `docs/specs/q2aw6-heater-pid-frontpanel-runtime/**`
- `docs/specs/fk3u7-frontpanel-input-interaction/SPEC.md`
- `docs/specs/223uj-frontpanel-ui-contract/SPEC.md`

### Out of scope

- Web 控制台、HTTP API、Wi‑Fi 配置写回字段扩展
- 多电压 / 多功率档位与自动 PD 策略切换
- RTD 额外校准界面或外部校准协议

## 需求（Requirements）

### MUST

- HIL host 的 warmup 满功率门禁以 `heaterOutputPercent=100%` 的逻辑命令为主，并要求软启动结束后 `heaterPhysicalOutputPercent >= 99%`。PPS 安全上限更新期间允许物理 PWM 在逻辑命令仍为满功率时短暂落在 `95%..99%`，但该瞬态连续时间不得超过 `2s`；低于 `95%`、持续超过 `2s` 或逻辑命令下降必须拒绝进入候选路径。这样既不把受安全限幅的短暂过渡误判为调优失败，也不掩盖真实的持续欠功率。

- heater 控制周期固定为 `50ms (20Hz)` 的真实单调时间目标，不得用主循环迭代次数或固定累加值虚构 elapsed time。每个周期聚合 `64` 次 RTD ADC conversion，并保留分数毫伏均值，因此 RTD 转换总频率为至少 `1280Hz`；`GPIO47` 在 PPS 与 fixed-PD fallback 全路径统一由 MCPWM 外设输出 `100Hz` PWM。PPS 高于有效 floor 时由受温度/电流合同限制的可调 PD 请求承担粗粒度功率控制并保持 `100%` PWM；PPS 已到 floor 或 bounded down-ramp 尚未到达 floor 时，PWM 必须按请求功率与当前电压等效功率之比连续延伸到 `0%`。每次首次 arm、目标切换或 `Approach -> Warmup` 回退进入预热时，物理 PWM 必须执行 `1000ms` 线性软启动。首次进入 HOLD 后，PPS 必须保持当前请求至少 `10s`，该窗口内只能由 PWM 对 PI 输出做快速响应；窗口结束后才允许以最多 `500mV` 的受限步进寻找工作电压。寻找目标是使 `100%` 物理 PWM 在目标温度附近仍有低斜率升温余量；一旦发现，不得因单次 PI 波动再次调压，除非持续满 PWM 且低于目标、升温率仍不足。运行时 status 必须回显上一轮控制起点间隔 `heaterControlIntervalMs`、RTD 至功率写入完成的执行耗时 `heaterControlCycleMs`、滤波升温率 `heaterFilteredSlopeCPerS` 与 `heaterCoastActive`，供 HIL 判定实际节拍、coast 残余热及恢复供热时机。
- heater 控制器必须按目标温区选择 ramp/approach/hold 参数，以及所有会显著影响 near-target damping 的调节量。`warmup` phase 的 current truth 固定为“沿温度-电压安全上限全功率预热”：只要状态机仍处于 `warmup`，输出就必须保持 `100%`，并让 `pps-mos` 后端持续贴着当前温度/电流合同允许的电压上限运行；不得再用 profile 中较低的 warmup power 把预热阶段降成功率受限的慢爬坡。warmup 退出距离必须取静态 `brakeDistanceCentiC` 与“滤波升温斜率 × `approachLeadTicks`”预测距离的较大值，并受 `warmupReenterCentiC` 滞回边界约束，不能在热惯性增大时仍固定等到静态刹车距离才降功率。Approach 段不得退回成“只要最终碰到目标就算通过”的静态功率下降；host 必须用 full-speed-to-stable、overshoot、hold p2p、source telemetry 与同步样本共同评价目标温区。保温阶段在 `holdPowerPermille` 基线上做连续 PI 微调；过冲时必须立即把输出降到 `0%` 并限制积分累积。预测滑行只允许在实际误差与滤波误差都已经落入该温区 `holdExit` 守门范围内时触发，不能因为单次预测提前把仍显著低于目标的温区拉成零输出。若 approach 已因预测储热切到 `0%` 后进入 hold，控制器必须保持 coast `0%`；只有滤波温度斜率 `<= -0.02°C/profile tick`、当前原始温度相对上一原始温度至少下降 `0.05°C`、实际与滤波低温误差都达到 `max(holdOnCentiC, 0.05°C)` 四项同时成立，才允许退出 coast 并恢复 PI。退出时必须清零 phase tick 与 hold-entry blend 残留，避免继承滑行前功率。hold 入口宽度不得同时放大滤波滞后参与 PI 的误差，滤波滞后补偿上限由 `holdOnCentiC` 约束。进入 hold 时必须允许把最后一次 approach 输出平滑 blend 到 hold PI 输出，并允许用可持久化的温区参数控制 `holdEntry / holdExit / holdOn / holdOff / overshootCutoff / holdKp / holdKi / holdBlendTicks / approachLead / holdLead`。用于 acceptance/HIL 的 saved profile 必须允许低温段使用更长的 approach 窗口，不能把接近目标的降功率阶段压缩到亚秒级；同一组全局 hold dynamics 不得被视为足够覆盖 `60~250°C` 全范围，低温与高温的 near-target damping 必须能够通过 API/EEPROM 控制数据独立收敛。
- host 工具必须把 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C` 视为默认 5A full-batch 的同等级调优目标集。物理目标集合始终按升序记录在 `tuningTargetsC`，默认实际执行顺序固定为递归二分的 `60, 240, 140, 100, 80, 120, 180, 160, 220`，并写入 `tuningExecutionOrderC`。调度规则固定为：先调当前区间两端点，再调中点；偶数区间取靠前中点；之后左半区优先、深度优先继续细分。每个目标温度的预算从冷却等待开始计时，单目标总 wall-clock 不得超过 `20min`；`targetTempC < 80°C` 时只能在 `currentTempC <= 35°C` 后启动，`targetTempC >= 80°C` 时只能在 `currentTempC <= targetTempC - 40°C` 后启动。默认 full-batch 不再额外采集 `0% / 25% / 50%` approach-only 曲线；这些曲线只可作为历史或显式诊断资料，不得进入默认调优流程消耗预算。每个目标的主闭环仍固定为 tuning scout、target-local retune、`current + 一个 evidence-specific predicted point` 的 batch compare，并在单目标预算仍有剩余时持续重复。短 scout 的 p2p 不得额外生成 hold-ripple candidate，任何 scout 或 batch candidate 只有同时满足 dynamic full-speed-to-stable gate、确认裕量、`maxOvershootC <= 3.0°C`、`holdPeakToPeakC <= 3.0°C` 与 `stage completed` 才能进入 `60s` hold confirm。默认 full-batch 流程不得再对调参轮次或 hold confirm 次数设置独立硬上限，唯一默认停止条件是 `completed`、`not_converged`、`candidate_ready`、真实 `budget_exhausted` 或 `environment_blocked`。区间内子目标只有在该区间两侧边界目标都已经 accepted 后才允许开始；未调过的目标只能用最近两侧 accepted 目标点做线性插值生成初始参数。任一目标一旦 accepted，其最终 point 参数必须冻结，后续流程不得回写或共享到其它目标；若某目标在预算内未 accepted，则该目标记为 failed，且该子区间不得继续细分，但其他仍满足边界条件的独立区间可以继续。`ThermalControlProfile` 的调优控制面参数必须 point-local materialize：`warmupReenterCentiC`、`holdEntryCentiC`、`holdExitCentiC`、`holdOnCentiC`、`holdOffCentiC`、`overshootCutoffCentiC`、`holdKpPermillePerC`、`holdKiPermillePerCTick`、`holdBlendTicks`、`holdReheatPowerPermille`、`approachLeadTicks` 与 `holdLeadTicks` 只允许从 point 生效；`settings` 只保留 `tempFilterAlphaPermille`、`approachMaxTicks`、`approachMinPowerRatioPermille`、`autoAdjustableWorkingFloorMv` 与 `heaterCurrentReserveMa`。导入旧 profile 时允许一次性 inflate 旧继承字段，但新写出的 profile、preview request、readback 对齐和 preliminary bundle 都必须使用完整 point-local 值。每轮 active self-test 的 host stage timeout 固定为 `180s`；remaining per-target budget 只允许裁剪冷却等待或阻止开始下一轮，不得把剩余预算反向压缩成更短的 stage timeout 或 warmup timeout。每轮必须回显 `selectedMode=100w`、`resolvedBank=pps5a`、`detectedSourceClass=pps5a`、source telemetry 与 full-speed-to-stable gate。preliminary review bundle 继续使用 canonical HTML bundle：`index.html + run.bundle.json + samples.ndjson + thermal-profile.accepted.json`；顶层必须回显 `kind=thermal_self_test_preliminary_bundle`、`bundleDisposition=preliminary_review`、`acceptedProfileRole=review_candidate_snapshot`、`tuningWorkflow=five_amp_batch`、`tuningTargetsC`、`tuningExecutionOrderC`、`temperatureSemantics`、`candidateDispositions`、`candidateReadyTargetsC` 与 `reportRuns`。`candidateReady=true` 必须同时满足存在可复盘 candidate point、存在有效测试证据、主展示结果 `stage completed`、`maxOvershootC <= 3.0°C`、`holdPeakToPeakC <= 3.0°C` 且 full-speed-to-stable settle time 不超过该目标动态门槛；不得仅因有 point、有样本或预算耗尽前曾产生过候选就标记 ready。每个 raw tuning entry 必须保留在 `targets` / `runs` 并包含 `budgetOutcome`、`timeSpentSeconds`、`roundCount`、`validTestCount`、当前生效参数、轮次参数与评价、温度/控制/source 图表。旧 `preliminary-review-*` 目录或既有 preliminary bundle 的 owner-facing 重写必须走 `flux-purr thermal report rerender-legacy` 这条 Rust CLI 路径。
- preliminary review HTML 的 `reportRuns` 只允许定义每个目标温度的默认主展示 entry，并且必须按物理目标温度升序排列；标题目标列表、summary card 与 target tab 必须使用同一升序目标顺序。页面不得因此省略 raw `runs`。HTML embedded data 必须包含完整 `rawRuns`，并在每个 target tab 内提供 raw entry 切换，使同一目标下该温度自己的 scout、batch candidate、hold confirm 等尝试的 samples、rounds、温度响应、控制输出、source telemetry、参数与评价都可见；`rawRuns` 保留实际执行/审计顺序，不强制按温度排序。
- per-target budget 必须是从 cooldown wait 开始、跨越 scout、每个 batch candidate 与 hold confirm 的单一单调 deadline；batch 内的 candidate 不得重置或穿透该 deadline。deadline 到期时，持有 lease 的 runner 必须先关 heater、释放 lease 并恢复 source，再以 `budget_exhausted` 收口该 target。
- preliminary review 的 owner-facing 结果只允许 `passed` / `failed` 二元分类。每个 raw entry 与 `reportRuns` 主展示 entry 必须包含 `reviewOutcome` 与 `reviewPassed`；`candidateReady=true` 或 `candidateDisposition=acceptance_passed` 映射为 `passed`，其它情况映射为 `failed`。`candidate_ready`、`budget_exhausted_without_candidate`、`environment_blocked`、`not_executed_without_accepted_bounds` 等内部分类只能保留在 JSON 审计字段中，不得作为第三种结果状态，也不得显示在 summary card 主结论区域。
- owner-facing 的 5A full-batch thermal tuning orchestration 必须走 repo-local `flux-purr thermal tune` Rust CLI；`flagship-tune` 只保留为兼容别名。历史 `scripts/thermal_tuning*` Python 入口已废弃并移出正式执行面；它们不得参与真实 HIL、正式调优、正式报告或验收证据生成。
- 真实 5A flagship HIL 的预检与恢复合同必须固定。启动任一目标前，host 必须先完成 repo-local tuning/report/dynamic-gate 单元测试，使用 repo-local `flux-purr-devd` 绑定明确授权的单一串口，并确认 Flux Purr readback 与 source readback 同时满足 `selectedMode=100w`、`resolvedBank=pps5a`、`detectedSourceClass=pps5a`、`100W`、PD enabled、PPS enabled、`pd_pps_5a=true`、`pps3_limit_ma >= 5000` 与 `tps_mode=auto_follow`。若 source telemetry stale 超过 `2s`、低压卡死、明确的 `SensorShort / SensorOpen / AdcReadFailed`、runtime reset 或硬件无响应，只允许执行同一 source 的 USB-C 断电再上电恢复：`isolapurr power runtime output --enabled false`、确认 `runtime.output_enabled=false` 且 USB-C 已不再出力（`usb_c_actual.status != ok`，或 `usb_c_actual.current_ma=0` 且 `usb_c_actual.power_mw=0`）、等待 `2s`、`isolapurr power runtime output --enabled true`、确认 telemetry 恢复推进，并在重新开始前再次确认授权串口仍是原路径。`runtime.output_enabled` 是 IsolaPurr 运行时电源门控的事实源；`usb_c_power_enabled` 只表示 USB-C path 连接/能力状态，不得单独作为掉电成功判据。掉电期间若 macOS 枚举出其它 Espressif 串口，host 只能记录为证据，不得切换授权端口。温度或 raw ADC 的变化幅度、方向、斜率或连续趋势不得单独触发恢复，也不得单独终止 live stage；`currentTempC`、`heaterFilteredTempC`、固件提供的 `heaterControlTempC` 与 `rtdRawAdcMv` 必须保留在 raw samples 并进入正式热指标。只有固件报告的传感器硬故障、过温、runtime/device、source telemetry、持续采样率故障，或连续超过 `2s` 的固件 `heaterControlMeasurementGuarded=true` 才可将本轮判为环境失败；后者必须保留全部原始样本和 guard 证据，并以 `temperature_sample_glitch` 明确收口。该原因不得仅由 host 根据温度变化推断。该恢复耗时必须计入同一目标的 `20min` 预算。
- HIL / self-test 的 full-speed-to-stable 硬判定按目标温度动态选择门槛：`targetTempC <= 150°C` 时，从首次离开 `warmup` phase 到首次进入稳定窗口不得超过 `10_000ms`；`targetTempC > 150°C` 时不得超过 `5_000ms`。稳定窗口定义为：后续连续 `10s`、采样频率至少 `3Hz` 的样本中，`abs(currentTempC - targetTempC) <= 1.5°C`；控制器可以在该窗口内短暂使用 `approach` 补偿热损耗，phase 只作为诊断记录而不是物理稳定性的必要条件。超过对应门槛仍未存在已经开始的连续稳定窗口时，host 必须立即停热并以 `full_speed_to_stable_timeout` 结束 stage。脚本必须在每个 stage 的 `fullSpeedToStable.limitMs` 中写出实际使用的门槛，不得依赖人工观察。
- self-test candidate 的在线识别不得在一次失败中同时搜索全部 profile 字段。脚本必须先按 full-speed-to-stable gate、是否进入 hold、overshoot、hold p2p、hold 高低侧误差、source telemetry 与同步样本归类故障，再生成候选。分类至少必须区分 `missed_lower_band_before_limit`、`missed_upper_band_before_limit`、`stable_window_broke_low`、`stable_window_broke_high` 与 `within_gate_low_margin`。`targetTempC <= 150°C` 的 full-speed-to-stable 门槛为 `10_000ms`，确认裕量为 `1_000ms`；`targetTempC > 150°C` 的门槛为 `5_000ms`，确认裕量为 `500ms`。裕量是短测候选排序与直接确认的优先信号，不是最终验收指标：短测已经满足动态 full-speed-to-stable 门槛、`maxOvershootC <= 3.0°C`、`holdPeakToPeakC <= 3.0°C`、stage `completed` 且存在 settle time 时，即使为 `within_gate_low_margin`，也只能进入一次完整 `60s` hold confirm；它绝不得直接成为最终候选或 `passed`。超过动态门槛、过冲超 `3.0°C`、hold p2p 超 `3.0°C`、stage 未 completed 或 settle time 缺失的短测不得进入 hold confirm 或最终候选。首次进入目标带后突破上界时必须增加刹车/lead 并降低低中温 Approach 能量；门槛时仍低于下界时只能渐进式减少刹车、提高 Approach 能量，不得把低中温刹车距离一步压到稳定带边界，也不得直接把 `approachPower / approachFloor` 拉到高功率上限。若已经存在有效 hold 样本且 hold p2p 超线，则不得让 full-speed failure 掩盖 hold ripple；低中温 hold confirm 的过冲或 p2p 失败必须 reseed 下一轮候选，方向为增加刹车/lead、降低 `approachPower / approachFloor`、降低过冲 cut-off 或 reheat 强度，而不是继续用同一短 scout 候选重试。`approachPower / approachFloor` 与 `holdPower / holdReheat` 是独立通道：尚无有效 hold 样本的 approach 失败不得抬高 hold 参数，`holdReheat` 不得被 `approachFloor` 强制抬高。采样、供电、通信或 runtime 故障不得修改 candidate。每次 candidate 更新后，所有运行时影响字段必须 materialize 到 profile 并通过 preview/save API 写入，禁止用替换固件的隐藏常量承载调参结果。
- profile preview 必须只驻留 RAM。`runtime_config.thermalControlProfile.op=preview` 需要完整 profile，`op=clear_preview` 清除 preview；`op=save` 需要完整 profile 并写入 persistent active thermal profile，`op=clear_saved` 清除 persistent active profile；状态回显必须暴露 `thermalControlProfilePreview` 区分当前是否处于 RAM preview。
- 低温目标发生 full-speed 低侧缺口时仍必须渐进调节；若缺口不超过 `0.5°C` 且该轮过冲不超过 `1.5°C`、hold p2p 不超过 `2.0°C`，允许把下一轮 approach 增量放宽到 `120‰`、刹车距离减少 `120c`、damping 减少 `180‰`，但仍不得一步提升到满功率，且候选必须重新通过完整安全门禁。
- CH224Q PPS 电压请求只按 `0x53` 的 `100mV` 单位对齐；AVS `25mV` 不作为首版 PPS 保温细分路径。
- 目标温度与 preset 写入都必须 clamp 到 `0~400°C`。
- RTD 开路、短路或 ADC 读失败时，heater 必须立即关断并进入测温 fault-latch；这些状态不进入蜂鸣或 attention 状态。`temp >= 420°C` 时进入独立的热失控 fault-latch 与 attention 状态。运行时不得依据 RTD 温度跳变幅度、升降方向、斜率或连续样本趋势推断 `sensor-discontinuity`；正常温度回落和快速升温都必须直接进入控制环。
- 测温 fault-latch 期间 heater 不得自动恢复；测温恢复有效后必须由用户重新 arm。热失控的恢复必须同时遵守绝对温度保护与告警确认规则，不得复用测温 fault 的通用重臂路径。
- CH224Q 在启动时默认请求 `20V`；`pd-request-12v` / `pd-request-28v` 仅改变默认固定请求值。随后必须读取 CH224Q power data 并只在 PPS APDO 覆盖 `20V` 时启用 `pps-mos`。固定 `20V` PDO 不得被当作 PPS 覆盖 `20V`。
- `pps-mos` 后端中，控制输出 `0%` 必须关 MOS；若 heater 仍处于 armed 加热会话，则请求回到有效工作下限并保持 `0%` PWM，只有 heater 真正关闭时才恢复 idle `12V` 或 source 宣告的更高 PPS 最小电压。控制输出 `1..100%` 必须映射到 `source PPS minimum .. safe_max_mv`，其中 `safe_max_mv = floor_100mV(min(V_source_max, I_source_max * R_estimated(T)))`，并继续受 PPS/AVS capability 上下限钳制；当请求已到工作下限或处于 bounded down-ramp 时，PWM 必须按请求功率与当前请求电压的等效功率比连续补偿到 `0..100%`。WARMUP 必须继续使用动态 PPS 电压表达控制功率，不得改成固定 PPS、电压一次建立后保持或用 PWM 替代其既有调压方案。自动控制单次同 APDO 请求变化必须限制为最多 `500mV`，以连续小步请求逼近目标；不足 `500mV` 的目标差异仍可抑制。相同 PPS APDO 内的电压变化不得关闭 MOS、不得制造等待期间的加热空窗；请求发出后必须等待至少 `25ms` 才可发出下一次同 APDO 小步请求，且在等待窗口内保留最新目标、不得用后续控制 tick 覆盖正在执行的 transition。只有 PPS/AVS 模式切换、固定 PDO/current-limit fallback、首次模式建立、失败降级或其它离散电源路径变化，才允许先关 MOS；大跨度或模式切换请求使用至少 `275ms` transition window，并在完成后恢复 gate。所有 CH224Q 可调电压请求在最终写寄存器前都必须 clamp 到不低于 `5V`；若 source capability 或上层控制请求低于 `5V`，实际请求必须提升为 `5V` 并记录 warning 日志。若加热时 `safe_max_mv < PPS minimum`，则必须临时请求固定 `9V` 并切回 `GPIO47` PWM，且 PWM duty 必须继续按 `I_source_max * R_estimated(T) / 9V` 钳制，直到 `safe_max_mv >= PPS minimum + 200mV` 才恢复 `pps-mos`；任一关键调压写入失败必须切回默认固定 PD + `GPIO47` PWM fallback。
- thermal profile 的 `autoAdjustableWorkingFloorMv` 默认 `5000mV`，可在 RAM preview 或保存 profile 中设为 `5000..28000mV`；运行时有效下限必须取该设置、source PPS capability minimum 与可用 maximum 的安全交集。低于 source capability 的设置不得形成实际请求。
- `GPIO47` 必须在全路径保持 `100Hz` MCPWM。逻辑输出 `0%` 必须立即关断 MOS；PPS 高于 floor 时保持 `100%` PWM，PPS floor 与 bounded down-ramp 区域按等效功率连续调节 `0..100%` PWM，确保物理输出不高于当前控制请求。不得恢复 pulse-density 或软件 GPIO 门控。
- 每个有效 RTD 批次的原始温度必须先独立执行 overtemp 判定；前面板与 owner-facing `currentTempC` / `boardTempCenti` 必须镜像当前有效样本。控制器使用单独回传的 `heaterControlTempC`：除 PPS request 的 `300ms` transition guard 外，仅允许以实际 `20Hz` 周期拒绝超过 `35°C/s` 的单样本物理突跳；该控制侧保护不得修改 raw/human 温度，不得生成 sensor fault，且必须回传 `heaterControlMeasurementGuarded=true`。未被保护的控制样本直接送入 `tempFilterAlphaPermille` 控制的 EMA，不得叠加跨 tick 多样本窗口、中位数、均值窗口或输出钳位。任一传感器 fault 必须立即清空控制器温度状态并保持既有加热关断路径。PPS request 发生变化时，控制温度保持最近可信值，直到 request 连续 `300ms` 未变化；恢复时必须同时重新播种 controller filtered temperature/slope，并重新使用经过控制侧物理门的稳定 RTD 样本。原始 short/open/ADC-read/overtemp 检查在该窗口内仍须逐周期执行。
- thermal batch 的默认调优目标固定为 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`，全部属于同等级调优集合；`250°C` 不参与默认参数调优。默认执行顺序必须采用“端点 -> 中点 -> 左半递归 -> 右半递归”的确定性流程，而不是简单低温到高温顺排。每轮开始前必须主动降温；重启阈值固定为：`targetTempC < 80°C => currentTempC <= 35°C`，`targetTempC >= 80°C => currentTempC <= targetTempC - 40°C`。温度首次达到或低于阈值后必须立即开始该轮，不得要求回到 `30°C`、不得增加固定 `30s` 或其它稳定等待，也不得跳过冷却。
- `I_source_max` 必须先取 PPS capability 与有效 CH224Q live current 的较小值，再扣除持久化/API 可控的 `heaterCurrentReserveMa`。默认 reserve 为 `200mA`，合法范围 `0..1000mA`；source readback、板级掉电或后续硬件差异允许通过 profile API 调整，禁止为了改变该余量替换固件。
- 手动 PPS 覆盖是非持久化调试状态，不写 EEPROM。启用时暂停自动 PPS/PID 电压写入，但 heater/PID 输出与 MOS gate 仍按既有逻辑运行；改压时不得主动干预 MOS gate。
- 手动 PPS 覆盖不依赖 `pps_covers_20v`，但必须存在 PPS APDO capability，目标电压必须在 capability 内、按 `100mV` 对齐且不高于 `21.0V`。CH224Q 写入失败或 PD 状态丢失时必须自动清除覆盖，回到默认固定 PD 请求或既有 fallback，并通过 status/trace 暴露错误。
- `active_cooling_enabled=true` 时，Dashboard fan line 必须只显示 `AUTO` 或 `RUN`；`active_cooling_enabled=false` 时必须显示 `OFF`，即使保护链路正在临时驱动真实风扇。
- Dashboard 中键短按只切 heater arm；中键双击切换主动降温（`active_cooling_enabled`）；中键长按只进菜单。
- `GPIO48` 蜂鸣器必须使用独立 PWM 通道；boot 和 idle 保持静音，不得复用 heater/fan 已占用的 PWM 输出。
- heater 成功切换必须播放 `heater_on / heater_off`；主动降温成功切换必须播放 `active_cooling_on / active_cooling_off`；heater 重臂被拒绝时必须播放 `heater_reject`。
- 任何已接受的前面板用户操作都必须有提示音；其中非 heater / 主动降温专用反馈的已接受操作（如菜单导航、子页进入/退出、预设编辑）统一播放通用 `ui_input` 提示音。
- 同一个蜂鸣器 cue 被重复触发时，必须从第一拍重新开始，不得沿用上一轮尚未结束的频率段。
- 过温保护不得占用 Dashboard 的风扇元素；SET 行必须在告警激活时以 `1Hz` 闪烁 `WARN / OTEMP` 两关键帧。
- `Active Cooling` 页面在正式 runtime 中为只读安全策略说明页；用户开启这一项时，口径统一称为“开启主动降温”，并必须同步默认 `20V`（及 `12V / 28V` build variants）、`40~60°C => 50% PWM`、`>60°C => 0% PWM`、`<40°C => 100% PWM + 30s`、加热期 `>100°C` 输出门控脉冲与 `>350 / >360°C` 包线。
- 当前风扇硬件为反相 `FB` 注入控制：`GPIO36 duty=0%` 表示最高风扇轨电压，`GPIO36 duty=100%`（`1000‰`）才表示最低风扇轨电压；所有 `minimum-voltage profile` 语义都必须落到该 `1000‰` 档位。
- 蜂鸣告警只允许存在两个 owner-facing 状态：`热失控` 与 `热失控待确认`。温度 `>=420°C` 的热失控期间必须每隔 `1s` 播放一次热失控提示；温度回落到 `<420°C` 后，若用户尚未确认，则进入待确认状态并每 `10s` 蜂鸣提醒一次。`SensorShort / SensorOpen / AdcReadFailed` 仍可停热并报告测温无效，但不得触发蜂鸣告警、待确认状态或 reminder。
- defmt 日志必须覆盖 RTD 读数、PID 输入/输出、heater backend 选择、PPS/AVS 请求电压、MOS gate 输出、fault 原因、fan policy 输出与 PD 状态变化。

### SHOULD

- cooling-disabled lock 的标签与恢复路径保持稳定，便于 monitor 与后续 review 收敛。
- 初始 UI 应在第一次有效 RTD 样本后就显示实际温度，而不是长时间保留 bring-up 默认值。
- firmware preview 与 Storybook 的 Dashboard/Active Cooling 文案和颜色层级保持一致。

### COULD

- 后续在同一条 PID 日志上扩展功率估算或 duty limit 观察字段。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- 启动后先请求 feature-selected 固定 PD 电压（默认 `20V`），随后读取 CH224Q status 与 power data。若 PPS APDO 覆盖 `20V`，heater 后端进入 `pps-mos`；否则进入 `fixed-pd-pwm-fallback`。
- 用户短按中键后，heater 进入 arm 状态；若无 fault-latch，则控制器按 `target_temp_c - current_temp_c` 输出 `0..100%` 控制量。`pps-mos` 后端先按 `3.2 ohm` heater load class 估算 `R_estimated(T)`，再用 `min(PPS APDO max current, valid CH224Q status current)` 计算动态电压上限；控制量 `1..100%` 仅在该安全上限内映射为可调 PD 电压并静态打开 MOS。若当前安全上限低于 PPS minimum，则临时改为固定 `9V` + `GPIO47` PWM，并继续按当前电流合同钳制 fallback PWM duty，待安全上限恢复后再回到 `pps-mos`。
- Dashboard 上/下短按和 hold-repeat 都只调整 `target_temp_c`，每次事件步进 `1°C` 并继续 clamp 到 `0~400°C`；中键 heater / active cooling / menu 语义不受 hold-repeat 影响。
- 用户双击中键后，切换的是“主动降温”策略位，而不是直接强制 fan GPIO。
- Dashboard fan line 只反映“策略开关 + 当前是否实际运行”：
  - `OFF`：风扇策略关闭
  - `AUTO`：风扇策略开启但当前无需工作
  - `RUN`：风扇策略开启且当前已使能输出
- 当 `active_cooling_enabled=true` 且温度位于 `40~60°C` 时，真实风扇必须使用 `GPIO36 duty=50%`（`500‰`）；当温度 `>60°C` 时必须切到 `GPIO36 duty=0%`（`0‰`）全速；当温度从 `>=40°C` 回落到 `<40°C` 时，真实风扇必须继续以 `GPIO36 duty=100%`（`1000‰`）运行 `30s`，然后才关闭。
- 当 heater 已 arm 但实时 heater 输出为 `0%` 时，`100<T<=350°C` 的普通加热期风扇脉冲必须关闭；当实时 heater 输出大于 `0%` 时，该区间的最低电压脉冲周期为 `5s`，占空比必须为 cooling-disabled 脉冲的两倍并封顶 `50%`。
- 当 `active_cooling_enabled=false` 且 `temp > 350°C` 时，heater 必须被强制关断并锁住；用户重新开启风扇策略或手动重新使能 heater 后才允许退出该锁态。
- 当 `active_cooling_enabled=false` 且 `temp > 360°C` 时，真实风扇输出升级为全速，但 Dashboard fan line 仍保持 `OFF`。
- PD 状态只做观测：即使 PD 丢失或降档，也不自动清空 `heater_enabled`。但 PPS/AVS 调压写入失败会把 heater 后端降级到固定 PD PWM fallback。
- 手动 PPS 覆盖激活期间，自动 heater backend 不再写 CH224Q 电压；固定 PD fallback 仍可继续使用 `GPIO47` PWM duty，`pps-mos` 仍可继续由 PID/MOS gate 表达加热输出。
- 温度达到 `420°C` 时，runtime 进入 `热失控`：立即将 heater 输出归零，并每隔 `1s` 播放一次热失控提示。用户可以通过前面板输入或 runtime/CLI/app 的 `faultAttentionAcknowledged` 确认收到告警；确认后停止待确认锁定与强制风扇状态，但温度仍为 `>=420°C` 时，绝对过温保护、停热和 `1s` 热失控提示不得解除。温度回到 `<420°C` 后，若告警已确认则恢复一般状态；若尚未确认则进入 `热失控待确认`，每 `10s` 蜂鸣提醒一次，并拒绝任何 heater arm 请求。强制风扇期间沿用现有主动降温包线：`>60°C` 全速、`40~60°C` 为 `50%`；温度 `<40°C` 或收到告警确认时结束强制风扇状态，两者任一先发生即可。风扇状态解除不等于自动重新 arm heater。

### Edge cases / errors

- 首次 RTD 采样失败时，heater 必须保持关断，直到后续有效样本恢复且用户重新 arm。
- 测温 fault-latch 期间若用户再次短按中键：
  - 当前测温 fault 仍存在时，必须拒绝重臂并保持 `heater_enabled=false`
  - 当前测温 fault 已消失时，允许清除 latch；该次输入只恢复可用状态，不得同时重新进入 arm
- 热失控告警未确认时，任何 heater arm 请求都必须被拒绝；温度 `<40°C` 只能解除强制风扇状态，不能代替告警确认，也不能解除 heater arm 禁止。
- 热失控告警确认后，若温度仍为 `>=420°C`，绝对过温 fault-latch 必须继续拒绝 heater arm；只有温度 `<420°C` 后才恢复一般状态，heater 保持关闭并等待后续独立的 arm 操作。
- `SensorShort / SensorOpen / AdcReadFailed` 期间 heater 必须保持关断；这些状态恢复后不得生成 `faultAttentionPending` 或蜂鸣 reminder。
- cooling-disabled lock 清除后，若温度仍高于 `350°C`，必须等待温度回到 `<=350°C` 再次越线后才允许重新触发锁定。
- `热失控待确认` 期间，第一次任意输入只能作为确认/静音；该输入不得顺带切 heater、切主动降温或发生页面导航。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `FrontPanelUiState.fan_display_state` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | Dashboard 风扇三态真相源 |
| `FrontPanelUiState.heater_lock_reason` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | `cooling-disabled-overtemp` / `hard-overtemp` |
| `FrontPanelUiState.dashboard_warning_visible` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | SET 行告警闪烁相位 |
| `FrontPanelUiState.manual_pps_enabled` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | Dashboard `PPS*` 调试覆盖提示 |
| `ThermalControlProfile` | USB/devd runtime config + persistent memory config | external | New | `docs/interfaces/http-api.md` | firmware / devd | CLI / devd / Web Serial | RAM preview 与 persistent saved profile，最多 10 个点；每点同时携带 power baseline 与 damping 字段 |
| `Status.faultAttentionPending` | USB/devd runtime status | external | Updated | `docs/interfaces/http-api.md` | firmware / devd | CLI / Web / tuning runner | 仅表示热失控已回落且尚未确认；测温 fault 不得置位 |
| `RuntimeConfig.faultAttentionAcknowledged` | USB/devd runtime config | external | Updated | `docs/interfaces/http-api.md` | firmware / devd | CLI / Web / tuning runner | 确认热失控告警；不得绕过 `temp >= 420°C` 的绝对停热与 `1s` 提示 |
| `FrontPanelRuntimeState` / `FrontPanelScreen` | TypeScript type | internal | Updated | None | web | Storybook / preview harness | 对齐 firmware 三态 fan 与告警关键帧 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given 固件刚启动，When RTD 已有有效样本，Then Dashboard 左侧显示实时温度，右侧显示 `SET/PPS/FAN`，其中 `FAN` 只会显示 `OFF/AUTO/RUN`。
- Given Dashboard，When 用户短按中键，Then 只切换 heater arm；When 双击中键，Then 只切换主动降温；When 长按中键，Then 仍进入菜单；When 长按保持上/下，Then 只连续调整 `target_temp_c`。
- Given heater 关闭且主动降温开启，When 温度 `39°C / 40°C / 60°C / 61°C`，Then fan 必须分别进入停止或 30 秒拖尾 / `50% PWM` / `50% PWM` / `0% PWM`。
- Given 主动降温已经把风扇拉起，When 温度跌到 `<40°C`，Then fan 必须以 `100% PWM` 再持续 `30s` 后关闭。
- Given heater 开启但实时输出为 `0%`，When 温度 `110°C`，Then fan 不得触发普通加热期脉冲。
- Given heater 开启且实时输出大于 `0%`，When 温度 `100 / 110 / 350 / 351 / 361°C`，Then fan 必须分别满足无脉冲 / `2%` 脉冲 / `50%` 脉冲 / `50%` / 全速。
- Given active cooling 关闭，When 温度 `100 / 110 / 350 / 351 / 361°C`，Then fan 必须分别满足无脉冲 / `1%` 脉冲 / `25%` 脉冲 / `50%` / 全速。
- Given active cooling 关闭且温度 `>350°C`，When 控制循环更新，Then heater 必须被锁住停热；When 用户重新开启风扇策略或手动重新 arm heater，Then 才允许离开锁态。
- Given `temp >= 420°C`，When 故障出现，Then heater 立即归零并进入 `hard-overtemp` fault-latch。
- Given 热失控仍活动，When 告警未确认，Then 蜂鸣器必须每隔 `1s` 播放一次热失控提示，任何 heater arm 请求都必须被拒绝。
- Given 热失控仍活动，When 用户确认收到告警但温度仍为 `>=420°C`，Then 确认可以清除待确认锁定与强制风扇状态，但绝对过温停热、fault-latch 与 `1s` 热失控提示必须保持。
- Given 热失控温度已回到 `<420°C`，When 告警此前已确认，Then runtime 恢复一般状态且 heater 保持关闭；When 告警尚未确认，Then 进入 `faultAttentionPending=true` 的热失控待确认状态、每 `10s` 蜂鸣一次，并继续拒绝 heater arm。
- Given 热失控已锁存强制风扇，When 温度为 `>60°C`，Then 风扇必须全速运行；When 温度为 `40~60°C`，Then 风扇必须以 `50%` 运行；When 温度回到 `<40°C` 或收到告警确认，Then 强制风扇状态立即结束；When 仅温度回到 `<40°C` 但告警仍未确认，Then heater arm 禁止必须继续保持。
- Given `SensorShort / SensorOpen / AdcReadFailed`，When 测温保护停热，Then runtime 必须报告测温无效并保留最后有效 owner-facing 温度，但不得蜂鸣、不得设置 `faultAttentionPending`。
- Given heater runtime 正常运行，When RTD 控制周期触发，Then 控制环必须以单调时钟按 `20Hz` 更新，每个周期聚合 `64` 次 ADC conversion，并把分数毫伏均值贯穿 calibration 与 PT1000 转换；RTD 转换总频率必须为至少 `1280Hz`。`tempFilterAlphaPermille` 默认值为 `750`，且必须继续由 thermal profile API/EEPROM 控制。status 必须同步发布 `heaterControlIntervalMs` 与 `heaterControlCycleMs`，且 HIL 原始样本必须保留这两个字段。
- Given native/Web Serial status 被读取，When 固件发布当前温度，Then `boardTempCenti/currentTempC` 必须直接由内部浮点 RTD 测量值四舍五入到 `0.01°C`，不得从前面板 `0.1°C` 显示值反推；前面板显示精度不得限制控制环或遥测精度。When RTD 进入 fault，Then owner-facing 温度显示必须保留最近一次有效读数，而不是写成 `0°C`。
- Given Dashboard 过温告警，When 页面刷新，Then 告警只占据 SET 行并以两关键帧闪烁，FAN 行不切换到告警文案。
- Given CH224Q power data 包含覆盖 `20V` 的 PPS APDO，When runtime 初始化 heater 后端，Then 选择 `pps-mos`；heater armed 且控制输出为 `0%` 时回到有效工作下限并输出 `0%` PWM，heater disabled 时才恢复 idle `12V` 或更高 PPS minimum；`1..100%` 只在 `min(V_source_max, I_source_max * R_estimated(T))` 允许的范围内请求 PPS/AVS 电压，达到 floor 或处于 bounded down-ramp 时由 PWM 按等效功率连续降低。对于 `3.25A` source，`0C / 20C` 下的自动加热不得直接请求超出电流合同的电压；对于更低电流 source，fallback duty 必须继续被压到不高于该电流合同对应的等效占空比。PPS 高于 floor 时 GPIO47 为 `100%` PWM。
- Given WARMUP 正在通过同一个 PPS APDO 动态调整功率，When 新目标电压与当前请求不同，Then 单次请求最多变化 `500mV`、MOS 必须保持导通且相邻请求至少间隔 `25ms`；只有 APDO/AVS/固定 PDO/fallback 等离散路径发生变化时才允许关 MOS，并使用至少 `275ms` 的 transition window。任何新控制 tick 都不得覆盖尚未完成的 transition。
- Given heater 正在运行，When RTD 温度出现普通升温、降温或可解释跳变，Then 样本必须进入控制环；当单样本变化超过 `35°C/s` 的物理斜率上限时，固件只允许把该样本保留为 owner-facing/raw evidence、暂不送入 PID，并回传 guarded 标记，不得产生 `sensor-discontinuity` fault 或伪造报告温度。When PPS request 正在按同一 APDO 分步变化，Then 控制器可保持最近可信温度到 request 连续稳定 `300ms`，随后用稳定样本重新播种 RTD 与 controller filter；该例外只由电源 request transition 驱动。只有 RTD 开路、短路、ADC 读失败或 `temp >= 420°C` 才可进入 RTD fault-latch，且这些原始安全检查不受 transition window 影响。
- Given hold predictive coast 已经激活，When 温度仍在上升、原始温度未下降至少 `0.05°C`、实际低温误差或滤波低温误差任一尚未达到 `max(holdOnCentiC, 0.05°C)`，Then 输出必须继续保持 `0%`；When 四项释放条件同时满足，Then 才恢复 PI，并从清零后的 phase/blend 状态开始。
- Given thermal batch 将运行目标 `T`，When 上一轮结束并主动降温，Then 若 `T < 80°C`，host 必须等待到 `currentTempC <= 35°C` 后立即开始；若 `T >= 80°C`，则必须等待到 `currentTempC <= T - 40°C` 后立即开始；不得附加低于 `30°C`、连续稳定时长或无冷却启动条件。默认 5A full-batch 正式目标集固定为 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`，全部属于同等级调优集合；默认实际执行顺序必须是 `60, 240, 140, 100, 80, 120, 180, 160, 220`；`250°C` 不参与默认调优。
- Given CH224Q 只提供固定 `20V` PDO 或 PPS APDO 不覆盖 `20V`，When runtime 初始化 heater 后端，Then 选择 `fixed-pd-pwm-fallback`，不得把固定 `20V` 误判为 PPS 可调能力。
- Given source 回报 PPS APDO capability，When 手动 PPS 覆盖启用为 `10.4V`，Then 自动 PPS/PID 电压写入暂停，MOS gate 不被设置动作额外改写，status 回显 manual/capability；When 覆盖清除、PD 丢失或写入失败，Then 自动控制恢复且错误码可见。
- Given `runtime_config.thermalControlProfile.op=preview`，When profile 含有最多 10 个槽位，Then firmware 只在 RAM 中启用 profile preview，目标温度落在点间时按 profile 线性插值所有 power/damping 字段；status 的 `thermalControl` 必须回显当前目标经过插值、旧格式 profile inflate（仅导入兼容时）和安全 clamp 后的有效参数、profile source 与 target coverage；When `op=clear_preview`，Then status 回显 `thermalControlProfilePreview=false` 且控制器回到 persistent saved profile 或默认曲线。
- Given `runtime_config.thermalControlProfile.op=save`，When profile 含有最多 10 个槽位，Then firmware 立即启用该 profile 并经现有 memory commit 路径写入持久化后端；编码必须只占用实际已配置点位，而不是强制写满 10 个空槽，避免 profile 扩展后挤爆现有 record 空间；When 设备重启后，Then 控制器继续使用 saved profile；When `op=clear_saved`，Then persistent active profile 被清除，RAM preview 不被隐式保存。
- Given `flux-purr thermal self-test` dry-run 或 mock devd，When 未显式传入 `--targets-c` 生成候选 profile，Then `targetsC` 默认只包含 `60 / 140 / 220°C`，不得包含 `300°C`；When 显式传入 `--targets-c`，Then `targetsC` 必须是标定网格 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 200 / 220 / 240°C` 加终端验收点 `250°C` 的有序子集，且仍不得包含 `300°C`。密集标定点允许使用相邻已配置目标点插值运行，最终保存的 profile 仍受持久化点数上限约束。
- Given 5A full-batch 调优正在执行，When 默认目标集为 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`，Then host 必须按确定性的递归二分顺序执行：先调当前区间两端，再调中点，偶数区间取靠前中点，之后左半区优先、深度优先继续细分。每个目标必须在 `20min` 预算内执行 tuning scout、target-local retune、`current + 一个 evidence-specific predicted point` 的 batch compare，并在单目标预算仍有剩余时持续重复同一 target-local scout/retune/batch/confirm 闭环；每当出现同时满足 warmup `100%`、动态 full-speed-to-stable gate、`maxOvershootC <= 3.0°C`、`holdPeakToPeakC <= 3.0°C`、stage completed 与非空 settle time 的 confirmable candidate 时，都必须进入一次 `60s` Hold confirm。确认裕量只决定候选排序和优先级；`within_gate_low_margin` candidate 必须通过同样的 `60s` Hold confirm，且在该 confirm 通过前不得标记为 accepted 或 passed。若 Hold confirm 以 target-local 热控失败结束但预算仍在，Then 必须保留该次证据、按失败方向 reseed 下一轮候选，并继续同一 target；低中温过冲或 hold p2p 失败的 reseed 必须降低 Approach 能量并增加刹车/lead，低侧 miss 的 reseed 必须渐进加速，直到 `completed`、`not_converged`、`candidate_ready`、真实 `budget_exhausted` 或 `environment_blocked`。When 某区间两侧边界目标都已 accepted，Then 该区间的中间目标必须以最近两侧 accepted 目标点的线性插值作为初始参数开始调优；When 某目标 accepted，Then 该目标的最终 point 参数必须冻结且不得再被后续目标回写；When 某目标 failed，Then 该目标所属子区间不得继续细分，但其它仍满足边界条件的独立区间仍可继续。默认流程不得额外采集 `0% / 25% / 50%` approach-only 曲线；历史曲线只可作为显式诊断资料引用，不得进入当前默认跑法。
- Given 真实 HIL self-test，When host 以 abstract bench source provider 启动自测并选择 `--profile-mode 65w|100w`，Then IsolaPurr source capability 必须读回 `65W` 或 `100W`、PD Fixed enabled、PPS enabled 与 `auto_follow`。65W 继续使用 `3.25A`；100W 使用 `21V/5A` preset。单候选通过后只保存到对应 bank；报告 bundle 包含 selected mode、resolved bank、detected source class、source preset/readback 和 save provenance，其中 owner-facing source-class display 固定为 `3A (65W)` / `5A (100W)`。
- Given 真实 HIL self-test 未显式提供 `--seed-profile-file`，When mode 解析到 `pps3a|pps5a`，Then host 必须从该 bank 的默认 seed 启动：`pps3a` 使用 accepted 65W baseline bundle 内的 `thermal-profile.accepted.json`，`pps5a` 优先使用 accepted 100W baseline bundle 内的 `thermal-profile.accepted.json`，否则回退到仓库内的 100W tuning seed；`auto` 只允许基于 configured capability class 解析 bank，不得为了 source prepare 而静默改成 65W preset。
- Given 真实 HIL self-test 准备启动任一 stage，When host 以 `heaterEnabled=false` 设定 target 后读取 `thermalControl`，Then profile source 必须为 `preview`、profile 必须覆盖该 target，且 host 必须使用与 firmware 相同的最近两侧已配置目标点线性插值及取整规则计算该 target 的有效 point；readback 中 point/settings 的每一有效字段必须与这组有效参数完全一致。物理目标点与区间内插值得到的有效点都必须在每个 stage arm 前独立执行该校验；密集验证目标不要求在持久化 profile 中存在同温度已配置点。任一字段缺失或不一致时必须在 MOS arm 前失败并执行 cleanup。
- Given 真实 HIL self-test 正在采样，When 固定 `3s` 窗口的实测滚动频率连续另一个 `3s` 宽限期保持低于 `3Hz`，Then self-test 必须记录单次间隔、滚动频率和电源快照年龄，以 `sample_rate_below_3hz` 停热并判失败；单次主机调度或串口停顿必须保留在样本中，但不得单独终止总体频率仍合格的 run。`--sample-interval-ms` 的请求值不得替代这一实测判定。IsolaPurr released CLI 采集不得阻塞 Flux Purr 控制采样循环；底层电源 telemetry 连续 `2s` 未推进必须判定 source telemetry stale 并停热。
- Given 真实 HIL self-test 写出 live run 工作目录或冻结 baseline bundle，When applied stage 已有原始 sample，Then `applied[].analysis` 必须带出 `approachSource` / `holdSource` 两段 source 统计，并在 owner-facing HTML bundle 的 stage 卡片里显示 source 电压、电流、功率的阶段平均值与范围；live run 工作目录只保留 `run.json` / `samples.ndjson` / `thermal-profile.candidate.json` 等数据文件，owner-facing 冻结 bundle 必须以 `index.html` + `run.bundle.json` 为 canonical 交付。调参结论不得只依赖温度曲线而忽略同步 source telemetry。
- Given 真实 HIL self-test 的任一 stage，When 控制器首次离开 `warmup` phase，Then 脚本必须开始统计 “full-speed-to-stable” settle time；若 `targetTempC <= 150°C`，门槛为 `10_000ms`；若 `targetTempC > 150°C`，门槛为 `5_000ms`。稳定窗口固定为后续连续 `10s` 内 `abs(currentTempC - targetTempC) <= 1.5°C`，不因 `heaterControlPhase` 在 `approach` 与 `hold` 间切换而清零。若在对应门槛后未存在已经开始的连续稳定窗口，脚本必须立刻关热并以 `full_speed_to_stable_timeout` 结束 stage；若窗口已在门槛内开始，脚本只允许继续采样至该窗口被验证或中断。报告必须写出 `fullSpeedToStable.limitMs`、`warmupExitedAtMs`、`stableWindowStartedAtMs`、`stableWindowVerifiedAtMs`、`settleTimeMs` 与 `failureReason`；runtime 掉电、重启、heater 意外 disarm、错误 target/mode 不得自动重臂后计为通过。
- Given preliminary full-batch review bundle 在 `100w / pps5a` 下执行，When 默认目标集合是 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`，Then bundle 必须包含 `index.html`、`run.bundle.json`、`samples.ndjson` 与 `thermal-profile.accepted.json`，并且 `run.bundle.json` 顶层与页面事实区都必须回显 `selectedMode=100w`、`resolvedBank=pps5a`、`detectedSourceClass=pps5a`、`tuningWorkflow=five_amp_batch`、`tuningTargetsC`、`tuningExecutionOrderC`、`temperatureSemantics`、`candidateDispositions`、`candidateReadyTargetsC` 与 `reportRuns`。`candidateReadyTargetsC` 只能包含 hard review metric gate 合格的 candidate；`maxOvershootC > 3.0°C`、`holdPeakToPeakC > 3.0°C`、stage 未 completed、full-speed-to-stable timeout 或缺失 settle time 的目标不得出现在 ready 列表。每个物理目标温度在 owner-facing 页面中只能出现一个 summary card 和一个 tab。每个展示目标 tab 必须按实际执行顺序展示该温度自身所有具备有效样本的 scout、batch candidate、confirm attempt，明确候选名、是否被采用、证据是否有效、失败分类、参数与评价，并提供对应温度响应、控制输出和 source telemetry 图。摘要卡必须以分钟显示该 target 的 wall-clock 总耗时，并显式显示所选实测轮次的逼近阶段用时；full-speed-to-stable 只允许在详情或诊断区作为稳定窗口 gate 指标显示。环境故障、未满足 accepted 边界而未执行的占位条目或被中止的非计划 attempt 可保留排除审计摘要，但不得混入 `validTestCount` 或候选评分。该 preliminary bundle 不代表 EEPROM saved bank，也不代表 committed accepted baseline。
- Given 同一物理目标温度存在多次 raw tuning attempt，When owner-facing HTML 打开该 target tab，Then 页面必须提供 raw entry 切换，且每个 raw entry 的 samples 与 rounds 必须存在于 embedded `rawRuns` 并可用于对应图表与轮次详情；默认主展示 entry 不得隐藏或删除同温度的其它失败/未通过尝试。
- Given preliminary full-batch review bundle 渲染标题、summary card 与 target tab，When 页面展示目标列表，Then 主展示目标顺序必须是物理温度升序 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`；不得按实际执行顺序、也不得按历史等级制顺序展示。
- Given preliminary full-batch review bundle 中某个目标以 `candidate_ready` 收口，When 该目标满足 hard review metric gate，Then owner-facing 结果必须显示为 `passed`；`candidate_ready` 只能保留在 JSON 审计字段中。Given 某目标为 `budget_exhausted_without_candidate`、`environment_blocked` 或 hard metric gate 失败，Then owner-facing 结果必须显示为 `failed`，且 summary card 不得显示这些内部分类。

## 实现前置条件（Definition of Ready / Preconditions）

- `flux-purr` 已完成 RTD 经验标定（当前按约 `3000 mV` 有效分压换算）。
- 前面板五向输入与现有 Dashboard / Menu 路由已可在真机上稳定使用。
- 板级 flash / monitor 统一通过 `mcu-agentd` 执行。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`
- `bun run check:devd`
- `cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- --json thermal self-test --device mock-fp-lab-01 --source-kind isolapurr --source-id iso-mock --source-url http://127.0.0.1:1 --dry-run`
- `source /Users/ivan/export-esp.sh && cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin flux-purr --release`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-pps-12v.framebuffer.bin --pd-mv 12000`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-pps-28v.framebuffer.bin --pd-mv 28000`
- `bun run --cwd web check`
- `bun run --cwd web typecheck`
- `bun run --cwd web test:unit`
- `bun run --cwd web build-storybook`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-fan-off docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-fan-off.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-fan-auto docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-fan-auto.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-fan-run docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-fan-run.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-overtemp-a docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-overtemp-a.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-overtemp-b docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-overtemp-b.framebuffer.bin`

### UI / Firmware Preview

- owner-facing 预览必须来自 `frontpanel_preview` 或 Storybook 的确定性输出。
- 视觉证据必须落在本 spec 的 `./assets/` 下，并和聊天回图保持同源。

## 文档更新（Docs to Update）

- `docs/specs/fk3u7-frontpanel-input-interaction/SPEC.md`
- `docs/specs/223uj-frontpanel-ui-contract/SPEC.md`
- `firmware/README.md`
- `docs/interfaces/http-api.md`

## Visual Evidence

- Dashboard fan `OFF`：

![Dashboard fan off](./assets/dashboard-fan-off.png)

- Dashboard fan `AUTO`：

![Dashboard fan auto](./assets/dashboard-fan-auto.png)

- Dashboard fan `RUN`：

![Dashboard fan run](./assets/dashboard-fan-run.png)

- Heating fan pulse gated by zero heater output：

![Heating fan pulse gated](./assets/heating-fan-pulse-gated.png)

- Heating fan pulse active with live heater output：

![Heating fan pulse active](./assets/heating-fan-pulse-active.png)

- Dashboard PPS `12V`：

![Dashboard PPS 12V](./assets/dashboard-pps-12v.png)

- Dashboard manual PPS override：

PR: include
![Dashboard manual PPS override](./assets/dashboard-manual-pps.png)

- Dashboard PPS `28V`：

![Dashboard PPS 28V](./assets/dashboard-pps-28v.png)

- Dashboard overtemp warning frame A：

![Dashboard overtemp A](./assets/dashboard-overtemp-a.png)

- Dashboard overtemp warning frame B：

![Dashboard overtemp B](./assets/dashboard-overtemp-b.png)

- Active Cooling policy page：

![Active Cooling](./assets/active-cooling.png)

- Current default temperature palette（Aurora / C）：

![Temperature palette current](./assets/temperature-palette-current.png)

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 落地正式 `HeaterController`、heater fault-latch 与风扇策略状态机
- [x] M2: 落地 Dashboard 三态风扇显示、SET 行告警闪烁与 Active Cooling 只读说明页
- [x] M3: 补齐单测、host preview、Storybook 故事与视觉证据
- [x] M4: 完成 xtensa build / review 收敛并准备 merge-ready PR

## 方案概述（Approach, high-level）

- 用单一 `HeaterController` 管理 PID 与 hard fault-latch，再把 cooling-disabled lock 作为独立安全层挂在 fan policy 旁边。
- 用 `fan_display_state + heater_lock_reason + dashboard_warning_visible` 作为 Dashboard 真相源，不再复用单布尔 fan 标记表达全部运行态。
- 用 `HeaterPowerBackend` 把控制器输出与硬件输出解耦：`pps-mos` 后端只做 MOS 静态通断并通过 CH224Q PPS/AVS 调压；`fixed-pd-pwm-fallback` 保留原 `GPIO47` PWM 调功。
- Approach 调优与验收以目标温度相关的 full-speed-to-stable gate、overshoot、hold p2p、hold 高低侧误差和 source telemetry 为真相源；默认 flagship sprint 不再额外采集 `0% / 25% / 50%` approach-only 曲线作为门槛。
- CH224Q 仍作为电源准备层而不是 heater interlock；只有启动 capability gate 与后续调压写入失败会影响 heater 后端选择。
- 两个 bank 的六个正式持久化目标点必须能与最长 Wi-Fi 凭据和完整校准状态同时持久化；EEPROM 使用 `1 KiB` v2 active 双槽，读取兼容 `512B` legacy 槽；EEPROM 不可达时 flash fallback 使用同一 record 编码和 sequence 选择规则。
- saved profile 与 USB/WebSerial direct preview 必须经过同一组 thermal settings 限幅；控制器不得依赖 devd 客户端校验来保护 spike-reject、工作电压下限或电流余量。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：当前 PID 默认参数仍需依赖实板热惯性验证，首次实现只能先选保守固定值。
- 风险：RTD 经验标定仍是经验值，不是外部标准校准；高温绝对精度仍可能需要后续单独处理。
- 风险：`0.2Hz` 风扇脉冲与半速 / 全速切换基于当前板级风扇 rail 映射，后续若硬件变更需重新验证。
- 假设：当前 heater 与 fan 硬件极性已经按现有 bring-up 经验验证为正确。

## 参考（References）

- `../223uj-frontpanel-ui-contract/SPEC.md`
- `../fk3u7-frontpanel-input-interaction/SPEC.md`
- `../../hardware/heater-power-switch-design.md`
- `../../hardware/s3-frontpanel-baseline.md`
