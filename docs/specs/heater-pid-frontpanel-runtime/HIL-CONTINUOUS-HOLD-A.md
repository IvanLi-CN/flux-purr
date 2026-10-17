# A组连续Hold实机资格

A组运行`firmware-pps5a-1792222845647`完成60、80、100、120、140、160、180、220°C八目标调优。标准报告的`ownerOutcome=passed`、`terminalDisposition=passed`、`reviewDisposition=complete`；八个目标均通过独立主机复核及连续60秒Hold确认，没有使用条件延长。合格确认的最大过冲为1.28°C，最大Hold峰峰值为2.74°C。

## 设备、产物与初始化来源

| 项目 | 实测身份或条件 |
| --- | --- |
| 加热台 | A组5.6cm，设备ID `288485681c38`，授权端口 `/dev/cu.usbmodem2111401` |
| 供电 | `~v~`，`f293cc9c139e`，100W／PPS5A能力，正常HOT=LOW |
| 固件提交 | `1a6918944385b020cec1a3c1d174ea17036de60b` |
| 应用版本 | `0.27.8-dev.1a69189` |
| ELF SHA-256 | `5c5beb647143fb8bab94f2d7e5d2b5cc43e6fea01a7b27a96f789d0155f759b8` |
| 固件功能 | 完整默认`esp32s3/web_serial/net_http/buzzer-test`，未裁剪功能 |
| 测温电气基线 | 分压供电3328mV，主人授权的临时假定ADC偏移−11000µV，保留eFuse及原始采样，没有填写人工用户ADC校准点 |
| 物理模型 | 设备当前有效标定事务`1346461204`，加热与自然冷却曲线 |
| 初始化来源 | `seed-calibration.json`的`device_active_calibration`；不导入历史调优参数，仅使用有效物理标定和同次运行反馈 |

保持现行温度、时间、保护和保存门禁。预热独立且逻辑请求100%；冷却期间持续全速风扇直至对应目标的起测温度门槛。测温基线是开发假定偏移，不构成独立绝对温度精度证明。

## 独立确认结果

稳定窗口起点按Warmup结束后的真实时间计算；该起点必须满足原期限，随后完整观察稳定窗口及独立60秒Hold。表中的过冲和峰峰值属于最终合格确认，不代替被拒绝试验的失败事实。

| 目标°C | 试验数，含确认 | 稳定窗口起点s／原期限s | 连续Hold s | 最大过冲°C | Hold峰峰值°C |
| --- | --- | --- | --- | --- | --- |
| 60 | 4 | 8.513／10 | 60.059 | 1.10 | 1.60 |
| 80 | 2 | 9.347／10 | 60.121 | 1.20 | 1.73 |
| 100 | 2 | 8.007／10 | 60.196 | 0.84 | 1.69 |
| 120 | 5 | 6.873／10 | 60.044 | 0.94 | 1.66 |
| 140 | 3 | 4.654／10 | 60.027 | 1.28 | 2.74 |
| 160 | 2 | 2.044／5 | 60.001 | 1.27 | 1.98 |
| 180 | 2 | 0.661／5 | 60.188 | 0.79 | 1.67 |
| 220 | 2 | 0.611／5 | 60.152 | 0.84 | 2.10 |

运行保留22个试验、7841个采样事件。全部试验的采样阶段历史均没有Hold→Approach/Warmup回退；八个独立确认均为`warmup -> approach -> hold`。完整事件0..8000通过设备轨迹验证，`deviceTraceVerification.verified=true`且拒绝原因为空。原始失败试验与搜索反馈保留，没有通过缩短确认、拼接Hold片段或修改旧报告取得资格。

## EEPROM与当前应用状态

候选`firmware-pps5a-1792222845647-candidate`原子保存到`pps5a`参数组revision74。`saveReceipt.atomic=true`、`readbackVerified=true`；候选哈希、RAM应用哈希及EEPROM读回哈希一致，均为`sha256:d9cf8b4db21c30a6cde960e577e702848088dd47ec83494c6cb47fc9b9ee10b7`。保存后实时状态为`profileSource=saved`、`profileActive=true`、`thermalControlProfilePreview=false`。

烧录沿用主人授权的`--skip-backup --confirm NO_EEPROM_BACKUP`方式，没有生成烧录前EEPROM开发备份，烧录前健康状态未知；revision74的写入与读回验证不能追溯替代该备份。测试结束停热冷却，32.35°C时确认`heaterEnabled=false`、逻辑及物理输出均为0，风扇已按正常空闲策略关闭。

## 启动栈缺陷与软件验证

前置提交`d33daa68`的两次运行分别只取得事件0和0..2即触发ProCPU栈保护重启，均未取得任何目标资格。任务入口栈检查遗漏了嵌套dispatch/read/status/page链路，完整链路仅余12044字节。提交`1a691894`将状态通过固定ABI逐字段写入独占堆目标，并通过`events_after_into`直接填充最终事件页，避免巨型按值返回栈帧叠加。事件协议、分页、确认、摘要、PD、热控算法和验收规则保持。

完整Xtensa产物的调用链余量35004字节，高于32768字节门槛；任务入口余量82380字节，嵌套PD后余量81036字节，高于65536字节门槛。Agent VM完成584项库测试、504项运行时测试、主机及Xtensa严格Clippy与完整release构建。应用大小2067808字节，小于原2097152字节分区。修复产物完成整轮实机调优和保存，未再出现重启。

## 可复核产物

- 最终报告：`firmware/target/hil-continuous-hold-a-1a691894-saved-bundle/index.html`。
- 机器可读结果与保存凭据：同目录`run.bundle.json`、`thermal-profile.candidate.json`。
- 原始运行：`firmware/target/hil-continuous-hold-a-1a691894-runs/firmware-pps5a-1792222845647/`，包含身份、标定初始化、采样、状态和决策日志。
- 精确产物及构建记录：`firmware/target/hold-phase-1a691894-a-offset11/flux-purr.elf`、`validation.json`。
- 主机封存轨迹摘要：`sha256:4a698334e252aeee00b5d90c54bd0602923e1021e2f2d307c0a7b3a6e703469b`。
- 设备轨迹摘要：`d40bcdba92fba6f176c9275632c2f0f8da6a7f39215588d70eb2696aba828ce0`；与主机封存摘要使用不同的摘要基础。

报告及原始运行位于本地构建输出目录，本文保留资格、身份和关键数值；历史违规或中断运行没有被覆盖或重新判定。
