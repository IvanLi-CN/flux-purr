# 标定余热交接的实机证据

## 结论与产物

完整余热预测、最晚Warmup交接、同轮制动搜索和PPS采样连续性修正已实现。A组在`19d8f76d96ea349696845e813081b05f808cb49e`完成八目标独立确认，候选经匹配的原子保存与读回验证写入EEPROM；历史失败记录仍保留原判定。实现与模型边界见[NATURAL-COOLING-CONTROL.md](NATURAL-COOLING-CONTROL.md)。

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

## 同映射重试的局部通过与搜索终态

运行`firmware-pps5a-1792121119665`继续使用完整固件`77c0366b`、3328mV和`−11000µV`映射，以及标定事务`1346461204`；没有导入上一运行候选。启动读数29.66°C，3795个样本中的最高原始温度为220.71°C，未复现上一轮276.38°C的异常。未复现不构成接触问题永久消失的证明。

60°C在trial6完成独立60秒确认并被设备接纳：过冲1.03°C、Hold峰峰值1.45°C，稳定窗口在Warmup退出后9789ms开始、连续10274ms后验证，满足原10000ms门槛，没有条件延长。初始短试验及中间候选的局部通过没有替代该次独立确认；整轮仍不能因此获得保存资格。

220°C的trial0短试验通过，过冲0.56°C、Hold峰峰值1.19°C、稳定窗口起点687ms。trial1独立确认记录过冲0.71°C、峰峰值1.14°C；trial2的不同候选仅将Approach lead从0改为1，记录过冲0.66°C、峰峰值1.15°C。两次失败均发生在原5000ms期限之后、连续窗口验证之前：PPS请求从20.5V恢复到21V时出现非接纳样本，窗口按既有规则中断。trial1的sequence3647为868505ms，距离Warmup退出5873ms；trial2的最后样本sequence3865为919100ms，距离Warmup退出6333ms。两次控制温度仍位于稳定带内，不能把这些热指标合格的短前缀当作完整稳定窗口或60秒确认。

搜索随后以`candidate_search_exhausted_no_distinct_safe_proposal`结束220°C，120/80/100/160/140/180°C均以`interval_pruned_without_accepted_bounds`跳过，没有实际运行。这是已运行目标的采样连续性失败及后续无新候选的搜索终态，不能归因为本轮出现异常高温，也不证明硬件无法加热到220°C。PPS等待样本不得跨接连续资格；要取得通过，仍需新候选的完整实测窗口与独立确认，不能冻结PPS或放宽门槛来改判现有轨迹。

0..3874共3875个事件连续，设备摘要与标准报告五文件摘要分别验证通过，标准报告trace摘要为`ef54aaf727d0c404da2053c927e87a1bb74b1081323fa9b718b515b779ac3896`，结果为failed／review complete。逐样本审计未发现全速冷却、正常Approach下限、固定Hold入口、标定初值或已接纳目标门禁违规；这不是八目标全部通过的证明。原始目录为`firmware/target/hil-fresh-calibrated-afterheat-a-77c0366b-retry/firmware-pps5a-1792121119665`，标准报告为`firmware/target/hil-fresh-calibrated-afterheat-a-77c0366b-retry-bundle/index.html`。没有EEPROM候选保存，终态读回heater disabled、MOS0、preview关闭、无手动PPS覆盖，保留全速散热至既有释放门槛。冷却后再次读回30.27°C、MOS0、无测温守卫或故障，风扇已按规则释放，合同恢复5V Fixed待机。

## 有界PPS采样暂停的资格连续性

固件`a9cecdfa35f8b75904e9a2d98b26a8dd680a036e`采用policy 5：已识别的PPS采样暂停只在原最大1秒接纳样本间隔内保留窗口起点，拒收值不成为样本见证；窗口仍须满足真实接纳样本密度、原始期限及完整10秒验证。CLI与固件共用连续性分类器，Web导出按相同规则复核，policy 2–4的历史结果不改判。相关固件、CLI、报告与Web共280项测试通过，完整功能release构建、原生目标严格Clippy及boot-stack检查通过；全库Rust样式检查仍受未修改的`calibration_cutoff_fixture.rs`解析问题阻断，不声称全仓检查通过。

原端口无备份自动烧录完成并读回`0.27.7-dev.a9cecdf`及完整提交身份；ELF SHA-256为`b9a068a46ab992384198763ce025062aedbea10774f0b4a9236ae0f6790c585d`，镜像占1792144/2097152字节，CPU0栈及嵌套PD中断余量分别为85340B、84748B。没有EEPROM备份归档，EEPROM整体健康状态未知；B组未被操作。3328mV、`−11000µV`、HOT=LOW、标定事务`1346461204`保持，运行`firmware-pps5a-1792125697895`仅以该标定生成初值，没有导入历史调优候选。

60°C的trial3完成独立60秒确认，过冲0.85°C、Hold峰峰值1.34°C、稳定窗口起点9106ms，原期限10000ms，无条件延长。220°C的trial1也完成独立60秒确认，过冲1.00°C、Hold峰峰值2.42°C；窗口在Warmup退出后674ms开始，连续10135ms后验证，原期限5000ms，无条件延长。220°C确认中，sequence2214–2216分别为525883、526012、526127ms的PPS恢复21V采样等待，控制温度219.46°C、原始温度219.74–219.78°C，`measurementAccepted=false`完整保留；这些拒收值没有增加有效样本数，也没有清空此前真实窗口。验证完成时间527084ms来自后续真实接纳样本。这直接覆盖了旧运行在期限之后因正常PPS等待错误清空未完成窗口的失败路径，局部通过不代替整轮八目标资格。

100°C的trial0暴露请求与合同异步生效的遗漏：sequence6688在1581219ms接纳99.49°C，请求已从13.5V改为13V而确认合同仍为13.5V；sequence6690在1581341ms接纳99.51°C并看到13V合同；sequence6691在1581458ms进入采样等待，控制99.51°C、原始99.62°C。只跟踪请求的分类器在中间接纳样本处过早清除边沿，导致下一条正常暂停清空稳定起点；此时已超过1581177ms原期限，试验立即失败，随后搜索耗尽。热端没有越出98.5–101.5°C稳定带，这一失败不能解释成实际失稳。`19d8f76d`同时跟踪请求与匹配的已确认合同边沿；实际时序的分类与运行时回归，以及CLI/Web一致性向量均通过，仍拒绝未知/不匹配合同、原始尖峰和超过1秒的暂停。该修正不改写原失败，须由新产物HIL重新建立资格。

该轮八目标全部实际运行，60/80/120/140/160/180/220°C完成独立60秒确认，仅100°C失败，整轮failed，未保存EEPROM候选。其它确认的过冲／Hold峰峰值／原期限内稳定起点分别为80°C：1.12°C／1.62°C／6935ms；120°C：1.28°C／2.09°C／3348ms；140°C：1.02°C／1.84°C／8055ms；160°C：0.95°C／1.80°C／2066ms；180°C：1.11°C／1.98°C／3313ms。事件0..9834连续，设备摘要独立核验通过，标准五文件报告摘要为`6cce6cefa7bb4cf2b0f65799fecbde9bd00bda1653b87b17e9ea6c5f9c0e97c8`，review complete；原始记录与报告分别保存在`firmware/target/hil-pps-continuity-a-a9cecdfa-full/firmware-pps5a-1792125697895`及`firmware/target/hil-pps-continuity-a-a9cecdfa-full-bundle/index.html`。终态读回heater disabled、MOS0、preview关闭、无手动PPS覆盖，全速散热仍在执行。

`19d8f76d96ea349696845e813081b05f808cb49e`的283项相关固件、CLI、报告与Web测试、严格Clippy、完整功能release及栈检查通过。ELF SHA-256为`ae6f40bc077280e4516e050b04c5608027e19c096c96e37a1cd2e68e8733b20e`，镜像1792128/2097152字节，CPU0及嵌套PD中断余量分别85340B、84748B。第一次烧录返回USB身份复核失败，随后只读枚举确认原精确端口和USB序列号仍在；应用读取超时后，同一端口的标准无备份烧录重试完成，设备读回`0.27.7-dev.19d8f76`及完整SHA，没有端口切换或EEPROM备份归档。启动后的首个测试尝试在加热前因缺少PPS能力停止；原始记录`firmware-pps5a-1792128293388`保留，不能视为温控试验失败或完整trace。对已授权A组电源`f293cc9c139e`的USB-C供电重新上电后，原端口及USB身份保持，设备恢复21V/5A PPS能力；当前Fixed待机合同不替代或限制该能力判断。

运行`firmware-pps5a-1792128448518`的100°C trial7完成独立60秒确认，过冲0.92°C、峰峰值1.50°C，稳定窗在Warmup退出后8007ms开始、连续10010ms后验证，原10000ms期限不延长。该窗口有40个真实接纳样本、3个拒收值，最大接纳间隔587ms。sequence9039在2128223ms接纳99.11°C并请求14.5V，确认合同仍15V；sequence9041又接纳99.52°C、合同仍15V；sequence9042–9044才观察到14.5V合同并进入采样等待，控制99.52°C、原始99.17–99.36°C。对应合同边沿使先前真实窗口得到保留，拒收值没有成为样本见证；sequence9045于2128911ms以真实99.42°C恢复接纳。它直接覆盖“请求后还有接纳样本、合同随后才生效”的遗漏路径，资格仍由后续真实样本完成。

## 八目标独立确认与EEPROM保存

运行`firmware-pps5a-1792128448518`以同一标定事务`1346461204`初始化，全部八个目标实际运行并完成独立60秒确认；没有导入历史调优候选。每个接纳试验的采样时序均由CLI根据原始样本独立验证通过，稳定窗口起点满足原期限，没有使用条件延长。trial编号从0开始，60/80/100°C分别在trial3/6/7取得最终确认；这些结果不构成“三轮内必然收敛”的证明。

| 目标 | 独立确认trial | 过冲 | Hold峰峰值 | 稳定窗口起点距Warmup退出 | 原期限 |
| --- | --- | --- | --- | --- | --- |
| 60°C | 3 | 1.00°C | 1.51°C | 8691ms | 10000ms |
| 80°C | 6 | 1.00°C | 1.45°C | 7964ms | 10000ms |
| 100°C | 7 | 0.92°C | 1.50°C | 8007ms | 10000ms |
| 120°C | 2 | 0.90°C | 1.71°C | 4690ms | 10000ms |
| 140°C | 2 | 1.03°C | 2.45°C | 4656ms | 10000ms |
| 160°C | 1 | 0.45°C | 1.44°C | 3961ms | 5000ms |
| 180°C | 1 | 0.99°C | 1.95°C | 2600ms | 5000ms |
| 220°C | 1 | 0.73°C | 2.62°C | 0ms | 5000ms |

设备返回`candidate_ready`后，使用标准`thermal confirm-firmware-save`对同一run、candidate及pps5a功率级执行seal、事件确认、原子保存和读回核验。保存回执为`bank=pps5a`、`bankRevision=64`、`atomic=true`、`readbackVerified=true`；candidateHash与bankHash均为`sha256:05dec9f8ebfaca2febc84cdda551773c512c075ab1026731c9ce6f4ff6734034`。完整事件0..11466连续，设备trace验证通过，标准报告结果为passed／review complete，trace摘要为`bf88e31cd6cc1a44599de2aa6f44f3da5e37d537fee51ec0f8d242450cfffe55`。

原始目录为`firmware/target/hil-pps-applied-continuity-a-19d8f76d-retry/firmware-pps5a-1792128448518`，匹配保存回执的五文件报告为`firmware/target/hil-pps-applied-continuity-a-19d8f76d-retry-saved-bundle/index.html`。保存后设备读回`profileSource=saved`、`profileActive=true`、`profileCoversTarget=true`、preview关闭、无手动PPS覆盖；停热全速散热后读到34.45°C、MOS0，风扇按既有门槛释放。B组保持未操作。

随后仅对A组已授权USB-C供电断电／上电，原串口及USB序列号保持，设备身份读回同一完整SHA、`resetReason=chip_power_on`。在始终停热状态下临时选择180°C，设备从`saved`来源读出的全部候选字段与报告180°C点一致，包括制动距离364、Hold基线423‰、Kp16和Ki1；目标随后恢复启动时的59°C。59°C低于60°C最低锚点，`profileCoversTarget=false`不表示保存失败。终态读回38.24°C、heater disabled、MOS0、preview关闭、无手动PPS覆盖、风扇关闭，Fixed5V待机，PPS能力仍为21V/5A。该读回证明断电持久化与参数加载，不构成新的测温精度或59°C温控资格验证。
