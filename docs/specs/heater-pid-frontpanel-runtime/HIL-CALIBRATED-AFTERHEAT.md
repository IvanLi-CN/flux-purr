# 标定余热交接的实机证据

## 结论与产物

完整余热预测、最晚Warmup交接和同轮制动搜索修正已实现，八目标HIL资格仍未取得，没有保存调优候选。实现与模型边界见[NATURAL-COOLING-CONTROL.md](NATURAL-COOLING-CONTROL.md)。

生产实现提交为`fde397adb9a96891bc835d8807dede17f02ad5bc`，PI初始化时间尺度文档为`b0e8ca74`，同轮制动反馈修正为`c6affb72e2f309756ebf4bcd5f4c1f83d25bee4a`。测试数据的ADC偏移反算修正提交为`205405b947896b706f4f8b215ab843421205aab0`。未导入历史调优候选，初值只来自设备绑定的标定事务。

`−20000µV`诊断产物`0.27.7-dev.205405b`曾烧录并读回，deviceId为`288485681c38`，精确端口为`/dev/cu.usbmodem2111401`。ELF SHA-256为`169ff2ec03f61f5b8a6550a7ef404ef64d53c047e92e3ab7e5d0d6a1841cf6d1`，完整功能镜像占`1790832/2097152`字节；CPU0栈余量85364B，嵌套PD中断余量84772B。成对无备份烧录在原端口重试后完成，espflash exit0；应用身份与完整提交匹配。没有EEPROM备份归档，EEPROM整体健康状态未知。B组未参与这些操作。

默认ADC偏移和`−20000µV`配置分别通过504项运行时测试；生产代码父提交的575项固件库测试和199项相关调优测试通过。完整功能release构建与boot-stack检查通过。这里没有声称整个仓库检查均通过。

## 八目标运行的实际边界

`firmware-pps5a-1792076620709`在`fde397ad`、假定ADC偏移`−11000µV`下运行完整八目标流程。60、220、120、80、160、140°C分别取得设备独立确认；100°C在单目标预算内没有得到合格独立确认，180°C过程中发生应用重启，整轮没有资格通过或保存。局部确认不构成八目标资格，也不沿用到后续ADC映射。

| 目标 | 独立确认过冲 | qualified Hold峰峰值 | 稳定窗口起点距Warmup退出 |
| --- | --- | --- | --- |
| 60°C | 1.18°C | 1.65°C | 8476ms |
| 220°C | 1.23°C | 2.51°C | 604ms |
| 120°C | 1.52°C | 2.16°C | 9434ms |
| 80°C | 1.35°C | 1.84°C | 7134ms |
| 160°C | 1.21°C | 2.34°C | 2600ms |
| 140°C | 1.47°C | 2.60°C | 4449ms |

该运行保留到sequence18199；重启导致RAM运行丢失，完整尾部无法验证，标准报告保持failed/incomplete。重启后立即读到`resetReason=chip_power_on`，不能据此单独断言供电中断、外部复位线路或固件根因。原始目录为`firmware/target/hil-calibration-seed-a-fde397ad-full/firmware-pps5a-1792076620709`，标准报告为`firmware/target/hil-calibration-seed-a-fde397ad-full-bundle/index.html`。

随后`c6affb72`的`firmware-pps5a-1792081133474`没有开始加热试验：零MOS、持续全速风扇命令下，读数在约14分钟后仍为36–37°C，未满足60°C起点≤35°C。该轮经标准取消，0..3405事件完整，摘要为`2f6bdd5577da006296f0fb835ea83fa9e05853c5cabb518b7e0b421185614110`；报告保持failed，没有保存。原始与报告分别位于`firmware/target/hil-calibrated-braking-a-c6affb72-full`和`firmware/target/hil-calibrated-braking-a-c6affb72-full-bundle`。

## 临时ADC映射与新标定拒绝

主人给定室温约30°C，并授权在偏差超过3°C时临时调整假定绝对ADC偏移。A组保留实测3V3基准3328mV，将假定偏移从`−11000µV`改为既有允许范围下界`−20000µV`；应用启动停热读数29.22°C，后续冷态读数31.90°C。没有创建用户校准点；RTD与VIN用户样本仍全部为空。这个室温假定不是独立测温精度证明。旧热模型按映射变化失效，不能继续作为种子。

新220°C加热／自然冷却标定run1从约31.86°C开始，获得122条原始轨迹，最高220.86°C，末点79.68°C；加热覆盖与冷却终点检查通过，但物理拟合没有通过。只读轨迹包含以下瞬态：

| 标定经过时间 | 报告温度 | 输出 |
| --- | --- | --- |
| 14.75s | 84.73°C | 100% |
| 15.25s | 73.80°C | 100% |
| 17.60s | 82.89°C | 100% |
| 18.10s | 141.77°C | 100% |
| 18.60s | 87.97°C | 100% |

这些瞬态与此前主人说明的接触不良相符，不能作为真实热响应。使用报告温度反算整数原始ADC格进行离线重放，既有非负物理参数求解的最低能量残差约20.68%，超过原20%阈值；该重放是拒绝原因诊断，不是新的标定或调优参数来源。设备返回`thermal_plant_projection_invalid`，没有安装新模型。未放宽残差、覆盖、瞬态守卫或调优资格条件；实验性拟合修改已移除。

完整只读分页证据位于`firmware/target/afterheat-offset20-failed-trace.json`，ADC用户校准导出位于`firmware/target/afterheat-offset20-calibration-failed.json`，产物与测试记录位于`firmware/target/afterheat-205405b9-validation.json`。这些本地诊断数据不构成标准八目标报告或保存凭据。

这轮最后一次读回为heater disabled、MOS0、31.90°C、风扇关闭，温度模型无效。它不是后续硬件状态或环境温度的保证。

## 恢复冷态映射后的重新标定与HIL

长时间停热后的冷态复核在`−20000µV`下读到26.33°C，偏离主人给定30°C室温超过3°C，因此没有沿用这个临时映射。恢复`−11000µV`后读到29.83°C。对应完整固件为`0.27.7-dev.77c0366`，source SHA为`77c0366bfa694b904697f99681be19dd505cd064`，ELF SHA-256为`f3303d585e02ad1d362f2ee3386e625ef1ad341ee1baa8660388f8de6e3a4a1d`。原端口自动烧录exit0并读回身份；23项标定回归、ADC反算测试、完整release构建和栈检查通过。没有修改生产控制律、3V3基准或用户校准点。

重新采集的118点220°C加热与自然冷却轨迹通过原拟合门槛，产生事务`1346461204`：热容量25075.047mJ/°C、对流122.005mW/°C、辐射5.271675e−7mW/K⁴、兼容纯延迟2150ms；完整停热响应纯延迟0ms、衰减4650ms、温度等效RMSE0.3836616°C、最大残差1.0304791°C。原始轨迹位于`firmware/target/afterheat-offset11-retry-complete-trace.json`。标定中仍记录到102.63→92.18°C的半秒跳变；拟合通过不能证明连接故障已消失，原始异常没有改写。

完整八目标运行`firmware-pps5a-1792120402692`只使用该新标定生成初值，`seed-calibration.json`绑定同一事务和完整停热响应，没有历史调优输入。尚处于60°C初始全速冷却时，原始温度从约57°C异常上升，sequence578／140498ms达到276.38°C；该样本`measurementAccepted=false`，守卫控制值保留为83.00°C。749个采样的逻辑请求和MOS全部为0、风扇使能全部为true且PWM均为0‰，其中212个样本被拒收。同步外部电源一次观测为21.060V／95mA／2.012W，不能把异常高读数解释为正常加热结果。

该轮在181469ms通过标准取消并完整排空事件，0..750连续，设备摘要验证通过，摘要为`9f9cec957047b7dc49bb1916936deecf1a6f06fdd80dc91f0eb4e263bc6473de`。没有开始物理加热试验、没有adopted目标、没有EEPROM候选保存。标准报告保持failed／review_incomplete，不把事件完整误作热资格完整。原始目录为`firmware/target/hil-fresh-calibrated-afterheat-a-77c0366b-full/firmware-pps5a-1792120402692`，五文件报告为`firmware/target/hil-fresh-calibrated-afterheat-a-77c0366b-full-bundle/index.html`。

取消后读回heater disabled、MOS0、无preview、无手动PPS覆盖，新热模型有效；原始测温仍异常，屏幕值不能作为真实台面温度。全速风扇命令继续保留，未用偏移或资格放宽掩盖接触故障。需要A组PT1000连接恢复稳定后，再核对冷态映射、按有效同映射标定启动完整八目标流程；未经完整独立确认和匹配保存回执，不能把当前实现或旧局部通过记录称为HIL通过。
