# LAN 设备会话与固件热控调优读取 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、验证和剩余缺口。

## Current Status

- Implementation: 无文件调优启动、LAN 身份存储、端点错误、owner proof、固件活动运行写保护及 Web 全局禁写已有实现；两份完整工厂基线、完整跨客户端验收、adapter parity 和完整调优验收仍待收口。现有控制参数种子不能当作完整工厂基线。
- 电源状态语义已明确：空闲时固定 5V/1A 属于正常安全状态；只有调优运行意图存在时才触发 PPS 协商。PPS 未确认前固件不加热、不采样，Web 应显示电源合同等待或失败原因，不得显示设备连接失败。
- 显式开始调优会建立待恢复的加热意图；实际输出仍须等待已确认的 PPS 合同，并继续通过传感器、过温、commissioning 和持久化安全门禁。调优进入 warmup 后，只有这些门禁全部满足才会驱动加热器。
- Web 固定 5V 提示说明这是正常空闲供电，启动后固件自动协商 PPS；未确认 PPS 只能说明暂时不加热，不应解释为无法发起调优。
- Lifecycle: active
- Catalog note: Web LAN session、tuning GET/POST、能力声明与有限读取已有实现；跨 adapter 错误一致性与完整运行验收尚未收敛。

## 中间升压与运行活动状态

- `702a8056` 的预热关断出现在中间合同 17.7V 和 19.6V：请求上限保持该合同，VIN 响应路径却仍选择更冷的原始点，并在电压资格判断时拒绝，触发 `flags=1`。响应检查与请求检查共享中间档位的 3°C 准入依据；只为响应比值选择对应的原始观测，不把请求提高到该点完整上限。最小回归在健康 VIN 下稳定失败、修正后通过；零 VIN 与超过窗口仍拒绝。完整 1,305 项固件测试通过，真机复测待完成。
- 固件 `be8bcfa6` 的 1,304 项固件测试、583 项 devd/CLI/integration 测试通过（另有 4 项显式 ignored），host/Xtensa Clippy 与样式检查通过。完整镜像经 `just firmware-image-check` 确认为 2,089,184 / 2,097,152B，ELF SHA-256 为 `e8faf66f6fccefef1912579242af309deeb6454fa3d81c3061d51178e84ea3d8`；LAN 栈余量 37,284B、嵌套 PD 余量 84,836B。授权端口烧录完成，HTTP identity 为 `0.30.10-dev.be8bcfa`；跳过 EEPROM 备份且健康状态未知。同一 Web 页面由 devd LAN HTTP 发起 `web-pps3a-702a8056-ae63-4cd4-806b-ab5b21f0269e`，活动模型 transaction `1470196405`，普通 HTTP status 读回 `thermalTuningActive=true` 与实际输出 100%。该轮 sequence 0–1369 共 1370 条事件完整，60°C 在 215.461 秒通过 60 秒保持确认，但 220°C 预热因实际输出中断，整轮在 399.675 秒失败且未保存。
- Web 经 devd LAN HTTP 发起的 `web-pps3a-3da26d45-eb63-4e51-8d71-3dd1891b6cc6` 使用固件 `5d1b5746`，完整收取 sequence 0–2987 共 2988 条事件。60°C 目标在 690.349 秒通过 60 秒保持确认；220°C 因预热实际输出中断，整轮在 874.669 秒失败且未保存。HTTP 状态读回 heater disabled、实际输出 0、29.81°C、固定 5V，不能把单目标通过当作整轮成功。
- 事件 1312 的已确认合同为 13.4V，但后端缓存仍为 12.9V，RTD 边缘的安全上限回到 13.0V。中间升压档位不匹配只识别完整工作点的回落保持逻辑。回归同时覆盖中间档位、协议确认领先缓存、未到边缘不得提前升压、超过 3°C 窗口必须回到冷边界；仅保留已准入档位本身，不提前提升到较暖观测的完整上限。真实匹配镜像复测尚待完成。
- 普通 status 提供 `thermalTuningActive`，USB/LAN 共用运行时 `armed` 来源，devd 透传该字段；旧固件缺少字段时兼容解码为 false。该字段不携带 owner proof，不读取或确认事件游标。Web 依据此字段显示页面级运行保护和安全停止按钮，预设温度、运行配置、手动 PPS、风扇策略、Wi-Fi、标定和固件安装写入停用，连接状态和只读浏览保持独立。预设温度待提交计时器在禁写生效时取消，停止不要求 mutation lease；解除运行保护后恢复原有权限判断。组件三个交互状态与整页 Dashboard / Settings 禁写回归通过，完整跨客户端矩阵仍待验收。

## 完整读取期限与失败展示

- 调优 GET 的期限覆盖连接、响应头和完整 JSON 正文。direct LAN 为 5 秒；Web → devd 为 12 秒，覆盖 daemon 的 3 秒排队和 6 秒设备读取预算。期限到达时取消请求并结束等待，即使 fetch adapter 忽略 AbortSignal 也会向页面返回可重试的 `transport_unavailable`；每次读取只发送一次请求，重试仍由页面的有限预算控制。成功、失败和超时均释放计时器；写入不适用读取期限，不自动重放。
- direct LAN 和 devd 成功 HTTP 响应的无效 JSON、null、标量和数组均返回不可重试的 `response_invalid`，保留操作、路径与 HTTP status。调优页不显示加热曲线标定模块的后台读取指示；调优错误显示在自身工作区。读取失败时不叠加正常 Fixed idle 供电说明。
- 响应头和正文停滞的四条 adapter 回归在修复前均未结束等待；devd 无效 JSON 回归错误地成功返回 null，成功 JSON 非对象的回归另复现五项失败。修复后 Web 406 项单元测试通过，期限回归同时检查没有请求重放、没有遗留计时器。54 项页面/组件交互测试、Web 类型检查与构建通过。端点超时控制不能证明设备网络连接恢复或完整调优通过。

## 真实 HTTP 长测与安全停止

- 固件 `58227acb` 的完整功能镜像经 `just firmware-image-check` 检查为 2,090,048 / 2,097,152B；LAN 栈余量 37,284B、嵌套 PD 余量 84,836B，1,305 项固件测试及 devd 测试通过。授权端口烧录完成，health 验证 `0.30.10-dev.58227ac`，ELF SHA-256 为 `5cfcf7bc38f5cda1a3f33e63236a1b4eb87df7655031483979984b67ce82450d`；跳过 EEPROM 备份且健康状态未知。
- 唯一本地 Web 页面经 devd LAN HTTP 发起 `web-pps3a-eb29c928-8a7e-4d57-8247-514e0bf1df0a`，sequence 0–3317 共 3318 条事件完整。60°C 在 213.711 秒完成独立 60 秒保持确认；220°C 的前三次预热没有实际输出中断，但各自在 180 秒内未到达接近阶段。第四次预热在 797.897 秒因实时安全上限降为 20.2V、已有合同仍为 20.9V 而实际关闭输出，整轮在 976.071 秒失败、未保存。该保护不得放宽；冷却到预热的工作点选择仍需可复现诊断。测试期间 Vite 开发刷新后继续同一 run，不能将其宣称为无页面刷新的长测。
- 随后通过 Web 页面级“停止调优”结束短测 `web-pps3a-5f1f1dc8-109a-47ab-bdbc-d9f46b6c9cad`，设备返回 canceled；之后 USB 状态确认 `thermalTuningActive=false`、实际输出 0、固定 5V。再次检查 HTTP 时旧地址连接被拒绝；USB 读回固件未重启，Wi-Fi 为 error、无 IP，错误为 `WiFi configuration could not be applied.`。不能把缓存中的旧地址或网络状态作为实时 LAN 连通证明。
- 通过授权 USB 端口向同一设备重新提交原有 SSID，省略 password、staticIpv4 和 telemetry 字段以保留已存配置后，原地址的公开 HTTP health 恢复且身份匹配；同一本地页面选择 devd LAN 后收取八条完整取消事件。该恢复不证明 Wi-Fi driver configuration failure 的根因已修复。标准桌面 1280×860 下，调优主体 viewport 为 583px、scrollHeight 为 583px，无横向溢出；事件列表使用自身的 SimpleBar 区域。活动运行与读取失败布局仍需独立实机验收。
- `eb29` 轨迹的同目标 warmup→cooldown 回放显示：安全上限仍为 21V 时，合同由 20.9V 降至 12.5V。`runtime_force_heater_safe_off` 在试验边界调用普通 idle 输出路径，将 PPS 后端缓存清回 12V；下一周期据此从 12.5V 重新升压，使全速冷却继续十多秒。活动调优的边界停热改为立即清零 MOS，保留供电后端与在途请求；下一正常输出周期继续执行实时边界和原有安全守卫。终止、失败和普通停热保持原有清理路径。该硬件运行时仅在 Xtensa 编译，host 测试不直接执行此入口；匹配镜像的转换轨迹与完整运行是修复验收依据，尚未通过前不能宣告已解决。

## USB 与 Wi-Fi 故障隔离

- `devdRecordToDeviceTarget` 只在 USB 本身未连接且没有设备 Wi-Fi failure code 时，兼容读取旧 daemon 放在 `network.lastError` 的串口故障；Wi-Fi 的 DHCP 或认证失败保留在网络状态中，不撤销已验证 USB 通道的配置能力。结构化串口错误事件继续独立显示。
- 回归覆盖正常网络、`ipv4_timed_out` 和 `association_rejected`：USB 仍已连接，有效 USB 租约下可以配置 Wi-Fi；既有串口超时、占用与缺失端口提示继续通过测试。
- 本地唯一 Web 页面在设备报告 `ipv4_timed_out` 时曾把 Wi-Fi 输入框错误设为只读；修复后的页面恢复可编辑。通过该页面的 USB 配置处理器原样提交已保存 SSID、保留密码和地址配置后，设备进入 generation 2 / sequence 15，随后报告 Wi-Fi 已连接；使用已有 LAN 凭据连接返回 HTTP 200。这证明配置恢复通路可用，不证明先前 DHCP 超时根因或完整调优已解决。

## 工厂基线覆盖审查

- `thermal_tuning::factory_default_profile` 从 `ThermalControlProfileConfig::default()` 填入八个目标点，仅返回控制参数，没有返回输出曲线、热模型或按来源组件组成的完整基线。现有无文件启动测试只证明控制参数种子存在，不能证明“设备完全没有标定数据时可用”。
- `lan::thermal_tuning_start_command` 仍通过 `thermal_model_heater_allowed` 检查设备已有参数；域层在缺少模型时返回 `thermal_model_unavailable`。尚不能按计划宣告完整工厂模型回退或 `thermal_parameters_unavailable` 契约已实现。
- `firmware/testdata/thermal-plant-final-hil-replay.json` 含已完成标定记录和原始追踪，当前 replay 测试验证其可拟合；`thermal_response.rs` 另有 220°C 关断响应的 HIL 样本。这些是候选数值来源，不能仅凭测试 fixture 身份宣布它们覆盖两档工厂发布所需的硬件适用性、来源追溯和安全边界验证。
- 两份完整基线、组件覆盖优先级、完整 profile digest 和无标定冷启动验收仍未完成；不得用单机已有标定时的启动成功替代这些验收。

## HTTP 接入诊断与定向验收

- 固件 `6b45399f` 经授权端口烧录并由 HTTP identity 确认为 `0.30.10-dev.6b45399`。本地唯一 Web 页面通过 devd LAN HTTP 完成整机供电边界和热模型标定：加热达到 220°C 后冷却，418.3 秒结束，持久化边界为 `wifiStarted=true`、27 个观测点；热模型读回容量 29,687.969、对流系数 228.28497、延迟 3,100ms。此前保存在设备上的 `wifiStarted=false` 边界不适用于 HTTP 使用时已经启动 Wi-Fi 的负载条件，自动控制因而使用理论模型兜底；不能通过取消 Wi-Fi 身份校验复用该边界。
- 同一镜像的 Web HTTP 调优 `web-pps3a-3977adc5-2728-478d-9e00-26fe6cb9a29b` 连续收取 sequence 0–1022 共 1,023 条事件，324.922 秒结束，60°C 与 220°C 均因预热物理全输出资格失败，其余区间剪枝，未保存。记录显示 13.6V 合同在标定上限从 13.7V 回跳到 13.0V 时保护关断。边界查找原先仅将精确等于标定电压的请求识别为已有工作点，遗漏自动控制预留的一个 100mV 档位；回归在旧实现下得到 13.0V 而非 13.7V。查找现兼容恰好低一个编码档位的工作请求，沿用已有 3°C 有界回落窗口；更低请求不能提前提升上限，超过窗口的真实降温仍降压并关断越界合同。匹配镜像的完整调优复测仍未完成。
- 同一标定记录在 RTD 1,134mV 时为 16.1V，在后续 1,145mV 时为 15.9V；原始逐点上限会先请求 16.0V，再因超过后续上限而关断。整表回放在该边缘稳定失败。自动请求上限采用同电流、同整机边界中后续较暖观测的最小值约束当前观测，避免消耗后续已知的更低上限；不会提前使用更高电压。原始点、供电身份、覆盖要求和 VIN 相对响应验证保持原样；3°C 回落窗口按解析后的工作上限及其单档余量识别已有请求，不能与仍按原始较高观测值匹配的旧语义混用。
- 固件 `addc41cb` 经授权烧录并由 HTTP identity 确认；完整镜像为 2,089,824 / 2,097,152B，ELF SHA-256 为 `5639168e2061c43a45c9eabbce9f96723f9d82b0aa1a2675b96a902620115151`，通过栈检查，按授权未备份 EEPROM。Web 读回 active calibration transaction `1470196405` 后，通过同一页面的 devd LAN HTTP 发起 `web-pps3a-69fe6e16-6baf-4e00-a19f-04baa280924d`。该轮连续收取 sequence 0–738 共 739 条事件，215.991 秒结束、未保存，仍因全输出资格失败；记录中的关断已变为大步保护 `flags=4`，分别包含 12.9→13.6V 和 14.4→15.2V 请求。Warmup 的直接跳压超过后端现有 500mV 连续调压界限；回归在生产关断判定函数中稳定失败。预热请求现遵守既有单步上限并逐步到达工作电压，越界旧合同仍立即降入安全范围，大步关断保护、静态全导通要求和评分门禁均保留。匹配镜像的真机复测仍未完成。

- `runtime_poll_control_transports` 将发送旧 USB 响应与执行新控制命令分开计费：旧响应背压、持久化日志积压和 USB 恢复期间仍允许 LAN mailbox 出队；新 USB 命令继续占用当前轮唯一命令预算。调度回归在旧行为下连续八轮未处理四个已排队 LAN 请求，修复后按序完成；另一回归确认新 USB 命令会将 LAN 延后到下一轮。1,293 项固件测试及 host/Xtensa 严格 Clippy 通过。匹配固件的真机 HTTP 和完整调优复测仍是独立门禁，不能仅凭调度测试宣告实际故障已解决。

- 镜像 `d037989c` 的并发公开 health 回归使用 `just lan-http-read-check http://192.168.31.207 d0cf1308a148 30 3`，30 次请求中 10 次成功、20 次连接拒绝；五组三并发探针均只有一个成功，单并发五次全部成功。故障位于单 acceptor 的 TCP 接入窗口，不能归因于配对或调优 capability。该路径依赖 Embassy 网络调度和设备 TCP 栈，host 路由单测不能替代匹配镜像的相同并发探针。
- Web 经 devd LAN HTTP 启动 `web-pps3a-3b40a1bb-99f4-41f5-a8fe-29775e331893` 返回 200，设备进入调优。临时诊断确认租约申请、写入和释放均返回 200，历史 `lease_busy` 未复现，原因仍未确定；诊断日志代码已移除。活动期间，另一已认证 HTTP 客户端持有自己的有效租约并提交保持原目标温度的 runtime 写入，返回 `409 thermal_tuning_active`，随后租约释放成功。Web 主动停止后，设备为 `canceled / armed=false`，sequence 0–1792 完整、无 gap；这是协议定向验收，不是整轮调优通过。

- 单 acceptor 改为三个独立监听 worker 后，90 次三并发读取仍出现 2–3 次拒绝，停止 devd 或把批间隔从 50ms 改到 200ms 都未消除。四个 worker 保留三个请求槽和一个收尾重叠备用槽后，隔离三并发 100/100 成功；Web 正常 HTTP 调优、状态轮询并行时，额外公开 health 读取 100/100 成功。该实验镜像基于 `da5ba372` 加备用槽改动，ELF SHA-256 `1406be6156f111a52137a3431d576c47bafa7ffaddc7d348c73c8263d69f5f8a`，镜像 2,084,368 / 2,097,152B，LAN 栈余量 37,284B、嵌套 PD 余量 84,884B。烧录完成且跳过 EEPROM 备份，EEPROM 健康状态未知。Web 无文件 HTTP 启动 `web-pps3a-2ab03b5e-0a74-4bfb-9eac-d852d51139d8`，进入 warmup/approach。持续运行至 sequence 2191 后，页面再次耗尽连接拒绝重试预算；手动恢复时设备为 `failed / armed=false`，trace.lastSequence=2448、gap=true，未保存。四槽改动改善短时接入，但不能据此宣告长时间 HTTP 可靠性通过；完整调优仍失败。

- 隔离页面后的状态/调优双读取探针各 30 次均返回有效 JSON，但完整响应分别最高耗时 3,307ms / 4,023ms，超过 devd 原有 3s 整体期限。分段 HTTP body fixture 在修复前以 `Body / TimedOut` 失败，修复后完整读取成功；原生已认证 GET（包括写入前 revision GET）单独使用 6s 整体期限，队列和 mutation 仍为 3s，不增加自动写入重放。真实长时间运行仍须复测，不能仅用该 fixture 宣告 HTTP 连续失败已消除。

- 本地 Web 经 devd `5b317a23` 的 LAN HTTP 运行 `web-pps3a-528fcd41-105f-4a5a-a719-193f2457c4ae` 持续超过 22 分钟，页面记录 sequence 0–4655 共 4,656 个连续事件。一次超时提示后自动读取恢复，恢复后 retry count 为 0，没有形成事件缺口；这不能证明所有超时已消除。事件 4074 / 1,200,105ms 为 `target_budget_exhausted_during_cooldown`，60°C 未通过便推进到 220°C，因此完整调优仍不通过。通过同一 Web 的停止按钮结束为 `canceled / armed=false`，trace complete=true、gap=false，digest `30f27806fdc6c9786a1c7969853432f7a1fb00a53ddc20e1b904debdb897bc7b`，未确认保存。随后真实设备 HTTP status 返回 200，heaterEnabled=false、heaterOutputPercent=0、network.state=connected。必须继续针对这份完整轨迹诊断候选搜索，不得把 HTTP 可持续读取当作调优通过。

- `calibrated_long_delay_plant_handoff_handles_sparse_measurements` 将该设备读到的 4,500ms 延迟、211.08168mW/°C 对流系数与 30,753.619mJ/°C 热容用于匹配的理想闭环模型，分别检验 28W、35W、65W 恒定可用功率与 500ms 有效采样间隔。在由模型计算的刹车起点下，控制器可在原有 10s 接近门槛内进入 Hold，峰值不越过原有 3°C 上限。该测试不回放真机的电压变化、采样噪声和候选搜索，也不证明工厂起点或整轮调优通过；它不能替代失败轨迹的可复现诊断。

## 纯延迟制动闭环回归

- 候选 `d5461c273eb6f514c1b9b5245ba295e8488d6e33` 的完整功能镜像为2,084,624 / 2,097,152B，ELF SHA-256 `841ebdd4c1b7b0cffd5bd256db64789edc39f581032d165f839842b047aa2317`；`just firmware-image-check`、启动栈检查、1,291项固件测试、host/Xtensa严格Clippy与Rust风格检查通过。授权端口烧录终态为complete，跳过EEPROM备份，健康状态未知；HTTP身份读回`0.30.10-dev.d5461c2`。原有本地Web页面经devd LAN点击开始，运行`web-pps3a-f9aebc40-e2c0-406e-b575-3aa068774e92`完成三次60°C候选试验，后二者进入物理Hold，但均未取得原时限内完整稳定窗口。约361秒后调优读取连续超时，浏览器已持久化前缀0–1232；手动重试补到1238，设备最终以`failed / armed=false`停热，trace.lastSequence=1489、gap=true、complete=false，digest `c06545b3000fbbbdb4bebcde28df9974fedafeb70289fc56a37116e3be13b8f3`。真实HTTP状态随后为heaterEnabled=false、heaterOutputPercent=0、mode=idle，无确认保存。该运行仍失败，不证明HTTP长期可靠、60°C资格或整轮调优通过。

- `factory_candidate_search_qualifies_a_calibrated_long_delay_plant` 将生产控制器与固件调优域状态机闭环连接，以该设备的4,500ms纯延迟投影、28W/35W/65W恒定功率和60°C单目标验证无文件候选搜索；28W另施加每250ms交替的±0.75°C已接纳测温扰动。保留旧斜率制动时该扰动场景连续十次试验耗尽接近时限，未接受目标；改为既有供热历史预测后完成scout及独立确认。测试保持原时限、真实全输出预热、物理Hold入口及3°C热指标，并断言不确认保存。冷却损失乘三是主动风扇场景假设，不是设备拟合结果；测温扰动不经过真实ADC/PPS守卫，不能冒充原始轨迹回放、跨目标或HTTP实机验收。±1°C扰动的探索仍未完成搜索资格，当前实机完整八目标调优仍未通过。
- `delayed_forecast_retains_heat_from_a_stronger_previous_pps_contract` 验证60W历史供热在未来能力降至20W后仍按原热量预测：原实现将预测63°C错误压成约61°C，修正后保留63°C；该测试只证明在途能量不被降档抹除，不提高未来请求上限或放宽保护。

## Implementation Coverage

- `REQ-LTR-001`: `web/src/features/control-plane-demo/lan-client.ts` 已能依据稳定 `deviceId` 合并 LAN 目标并更新地址；`storeLanDeviceSession` 按身份键保存并兼容旧地址记录；`resumeLanDeviceSession` 在匿名 health 身份匹配后复用凭据并迁移 locator，身份不符不发送 bearer。
- `REQ-LTR-002`: `lan-pairing-panel.tsx` 已有已保存凭据恢复和 401 重新配对入口；需要确保调优读取失败不会删除凭据或打开配对面板。
- `REQ-LTR-003`: `control-plane-demo.tsx` 已把调优读取放入独立回调；`FirmwareTuningWorkspace` 使用自己的读取状态、快照和重试预算；真机已观察到顶部温度持续更新而调优读取停止，完整 fixture 矩阵仍需验收。
- `REQ-LTR-004`: `firmware/src/net_http.rs` 已注册 `/api/v1/thermal-tuning` GET/POST；`firmware/src/control_plane.rs` 在 `web_serial` 或 `net_http` 条件下声明 `thermal_tuning`；Web 和 devd 将调优路由的 404 归一化为 `thermal_tuning_unsupported`。
- `REQ-LTR-005`: LAN client 已有无租约 GET 回归测试，调优 start/seal/confirm-save POST 沿用 lease，已认证 cancel 除外；固件网关区分只读、普通写入和安全停止。
- `REQ-LTR-006`: `lan-client.ts` 与 `transport-client.ts` 已携带 operation、endpoint、HTTP status 和 category，区分 401、调优 404、5xx、无效 JSON 与传输异常；devd LAN bridge 的传输错误输出中文原因，并保留 `transportKind`、`ioKind` 和 `osCode`。只导出类型化原因，不输出可能含请求 URL、凭据或查询值的原始错误链；JSON 解码错误为不可重试的 `response_invalid`，不归类成连接失败。完整跨 adapter 契约仍待验收。
- `REQ-LTR-007`: `firmware-tuning-workspace.tsx` 的空闲和活动运行共用有限读取预算；连续三次可重试失败后保留最后快照与游标，停止轮询并提供手动重试动作。活动运行不得自动清零失败计数。对最后观察到的活动运行，安全停止不依赖读取重新确认或本地日志就绪；其他写入仍受原有门禁保护。
- `REQ-LTR-008`: 当前错误显示复用通用 `messageOf`，需要加入调优操作、响应类别和安全动作的中文消息目录。
- `REQ-LTR-009`: 全局日志已有设备事件面板，但调优读取尝试尚未统一记录 endpoint、结果类别和 retry count；敏感字段必须继续保持 redaction。
- `REQ-LTR-010`: Web Serial、devd 和 direct LAN 已有独立 adapter；需要使用同一 tuning domain state 和 fixture matrix 验证 transport parity。
- 调优使用生产 `AutomaticThermal` 电源请求。高优先级 PD 服务为同一已确认 APDO、同一合同电流内的健康 PPS 电压步进保留 heater permit，避免低优先级控制周期恢复输出前被异步强制关断；能力刷新、合同范围/电流变化、拒绝、超时和低压互锁仍关断。预热实际全输出资格不放宽，零输出帧继续使该试验不合格。
- 自动 PPS 的预热、接近、保持与供电准备工作点持续保留一个 100mV 上限量化档位的余量；合同保持分支与 Hold governor 使用同一工作上限，不能消耗该余量而重新贴住硬安全上限。健康的小步调压保持输出，真正越界的合同仍立即关断并迁入安全范围，物理全输出资格保持不变。确认合同位于实时上限下方一个 100mV 档位内时即可进入预热，不要求精确等于上限。
- 已确认的 PPS 合同可能领先后端缓存的请求值；重同步是否关断须依据实时安全电压上限，不能把低于当前合同的目标请求值当成安全上限。模式变化和实际越界仍关断；同一安全范围内的重同步保持物理输出。
- devd 活动会话和调优请求绑定页面选中的具体 bridge 目标，不得在显示 LAN 时借用同设备 USB 目标的租约。调优 GET 在 Web、devd USB 和 devd LAN 间透传 `run_id` 与完整宽度 `after_sequence`，包括零值；缺少游标会重复返回旧页、无法确认已收事件，最终耗尽设备事件缓冲并使运行失败。
- 固件事件确认保持单调且幂等：较新请求已确认并回收事件后，较旧请求重复已确认游标不会制造全局追踪缺口；未交付事件的未来确认仍被拒绝。Web 只因自身的记录失败或缺口自动停止本页发起的运行，观察者仍可手动安全停止。
- HTTP socket 由四个固定 worker 独立监听、处理和复用（三个请求槽和一个收尾重叠备用槽）；不得只由单个 acceptor 接入后再移交，否则同一网络轮询中的并发 SYN 会在其他缓冲空闲时仍被拒绝。回收必须结束连接两端后再重新监听。`close()` 后的 `flush()` 只等待本端 FIN 确认，不能证明对端已关闭；处于 `FIN_WAIT_2` 的 socket 无法重新 `accept()`。响应正文完整发送并获得 TCP 确认后直接 abort 并有界等待 reset 发出，不为对端 half-close 占用有限 socket，避免并发轮询期间失去监听能力。只读回归入口为 `just lan-http-read-check <device-origin> <device-id> [requests] [concurrency]`，不携带凭据、不跟随重定向，身份不符立即停止。
- devd 对同一规范化 HTTP origin 的已认证请求共享两个在途槽，其余请求公平等待，等待超过 3 秒返回可重试的 `503 server_retryable`。这是请求调度预算，不限制客户端数量或取得控制权；不同 origin 使用独立预算。写入前用于读取 revision 的 status 响应必须完整收取，再发送写入，避免丢弃正文占用设备连接。八个并发 HTTP 读取的回归测试在限流前超过两个在途请求，限流后通过。
- `REQ-LTR-012`: `thermal_tuning.rs` 已编译进 `pps3a` 与 `pps5a` 两份控制器起点 profile；无上传候选时可以解析匹配起点，并保留独立安全门禁。输出模型、热模型初值、安全边界与工厂数据来源的完整性审计尚未完成，不能将控制器 profile 等同于完整工厂基线验收。
- `REQ-LTR-013`: 启动时设备候选优先、缺省时工厂基线兜底；候选仍保持 RAM 预览并按功率档位保存。Web 高级导入接受 `pps3a` 和 `pps5a`，校验文件的 `powerClass` 与包装层 `profile.bank` 一致，并在启动时再次核对设备当前解析的档位；档位变化或不匹配时拒绝发送启动请求。预览命令使用匹配的 bank。两档均保留固件的历史内容哈希编码 `thermal-tuning-v2/candidate/pps5a`，哈希不代替独立的 bank 和功率档位校验；普通无文件启动继续省略 `candidateProfile`。
- `REQ-LTR-014`: 调优停止在固件 HTTP 网关、排队命令执行、devd LAN bridge 与 direct LAN Web 中作为已认证安全例外执行，不获取 mutation lease、不读取 optimistic revision；重复停止保持已停机状态和原有终态证据。start、seal、confirm-save 和其它写入继续受原有租约与 revision 保护。固件成功启动时生成 owner proof，仅在启动响应返回；只读状态构造为不含证明。保存入口在 EEPROM 写入前校验证明，运行域在提交回执前再次校验；停止、失败、新运行和成功保存均失效。devd USB/LAN 透传证明并在诊断输出脱敏；Web 启动命令将证明隔离在发起页面的 RAM/sessionStorage，普通状态进入 React/journal/report 前移除证明，确认保存时才注入请求；其他页面保存按钮不可用，停止仍不要求证明。失败、停止、替换运行和成功保存清除页面证明。匹配镜像的浏览器保存验收仍待完成。
- 活动运行在固件共同 USB/LAN 命令分发处拒绝客户端写入并返回 `thermal_tuning_active`；只读和调优安全停止仍允许。判定覆盖运行配置、Wi-Fi、标定、持久化、加热曲线、其它调优写入和蜂鸣器写入；蜂鸣器状态读取仍允许。devd 将该 USB 协议错误保留为 HTTP 409，避免误报为通信网关故障。Web 全局禁写提示与置灰已实现，匹配镜像的完整跨客户端矩阵仍待完成。

## Verification / Rollout

- 镜像 `d037989c`（ELF SHA-256 `928f9f8dc4fd961dcc12029135909ece53b4cfc8e8e9c223a8c8738627fd8956`）通过 `just firmware-image-check`：2,085,168 / 2,097,152B；LAN 栈余量 50,820B、嵌套 PD 中断余量 98,420B。经授权端口烧录完成，公开 health 读回 `0.30.10-dev.d037989`；跳过 EEPROM 备份且健康状态未知。Web 重连后首次启动未发出 HTTP POST：旧的显式 LAN 选择与后台 USB 租约不一致，写入错误随后被只读成功清除。桥接连接完成回调已同步具体目标，写入错误与读取错误独立保留；同页复测确认 USB 重连显示 USB，再选择 LAN 时状态轮询也转为 LAN。
- 选中 LAN 后的启动已真正发出 HTTP POST，但固件返回 `409 lease_busy`，超过声明的租约 TTL 后重试仍相同；设备保持 `idle / armed=false`。页面后台 GET 持续 200，写入错误仍保留。此证据尚不能确定占用者或续租来源；当时旧游标的真机重放与整轮调优尚未通过，不以编译和短测替代。
- 延迟读取者回归测试在修复前因旧游标将追踪误标为 gap 而失败；修复后通过，未来游标拒绝路径保留。681 项固件库测试、607 项运行时测试、严格 host/Xtensa Clippy、387 项 Web 测试、typecheck、Biome 和生产构建通过。匹配镜像的旧游标重放已通过：Web 收到 sequence 236 后，另一客户端请求 after_sequence=0 返回 200 且 trace.gap=false，运行继续。该回归不能单独证明此前短测缺口的原因。
- 后台读取期间的启动按钮错误禁用已在真实页面复现：设备已识别且空闲时，2 秒内 81 次观察有 28 次禁用；移除该读取态门禁后，同一页面 81 次观察均可启动。首次身份读取、调优活动和命令执行的门禁保留；命令开始前已发出的读取响应不能覆盖命令后的状态。
- 显式选中具体 LAN bridge 时，路由恢复与 live devd lease 必须保持相同 targetId。缺少 LAN 租约期间不能切回同身份的 USB 路由，否则界面显示 LAN、写请求却没有对应租约。回归测试在修复前返回错误 fallback，修复后通过；全量 387 项 Web 测试与 typecheck 通过。
- 完整功能镜像 `62bed0c071a1d167`（ELF SHA-256 `bfa5016fc686f64f4013fee942cb88cb92ad09933480e8d2b42ee6af67f11896`）通过 `just firmware-image-check`：2,085,152 / 2,097,152B；LAN 栈余量 50,820B、嵌套 PD 中断余量 98,420B。经 repo CLI 在主人授权端口烧录完成，跳过 EEPROM 备份且健康状态未知；启动后 USB 与 LAN 身份均读回该 build。
- Web 通过 USB 辅助认证获得同设备 LAN 连接；在当前唯一页面选择 LAN 后，无文件启动运行 `web-pps3a-ab66b5b3-ccb8-4a1c-86ca-f5249174ee1b`。HTTP POST 为 200 且仅启动响应含 owner proof；后续 GET 为 200、不含证明，事件游标连续到 sequence 50。协议短测后通过 Web 停止，设备读回 `canceled / armed=false / saveConfirmed=false`，sequence 0–53 完整、无 gap；页面证明已清除，IndexedDB 运行记录不含 ownerProof。此证据只覆盖启动、读取、证明隔离与停止，不是整轮调优通过。
- 同镜像此前短测运行 `web-pps3a-4364736c-29c3-48b5-b136-b49d8b751432` 出现 `device_trace_gap` 并自动停止；后续捕获 HTTP 顺序的短测未复现。该瞬态缺口原因仍未确认，不以一次短测通过宣称已解决。
- Web 发起者证明保管、同页刷新恢复、不同页面隔离、替换运行失效、终态清理和仅保存命令注入证明已有 9 项测试；相关 Web Serial、transport 和调优工作区共 63 项测试通过，typecheck 与 production build 通过。真实设备已烧录保存证明协议，完整保存流程仍需匹配镜像的单独验收。
- 固件 `014d8eb210bfc3f2` 的 Web 经 devd LAN HTTP 运行 `web-pps3a-e9e7fce2-0ab9-48eb-90e0-f9be67e6ef11` 在约 1744.4 秒后终止，连续收取 sequence 0–6271 共 6272 条事件；终态为 `failed / armed=false / review=complete / trace.complete=true / gap=false`，未保存 EEPROM。60°C 试验未通过，220°C 在预热全输出下于约 108°C 触发 `trial stage timeout / warmup_timeout_without_safe_warmer_retry`，其余目标以 `interval_pruned_without_accepted_bounds` 跳过。这证明该轮 HTTP 事件收取完整，不证明调优成功，也不能排除其它运行中的间歇传输故障；控制阶段时间预算与实际热响应仍需排查。
- 保存证明回归测试在修复前以公开候选信息成功保存而失败；修复后，缺少证明的保存被拒绝且候选状态不变，合法发起者仍取得包含 bank revision/hash 的回执。680 项固件库测试、607 项运行时测试、devd 全量测试、Rust structural check、host/Xtensa 严格 Clippy 通过。证明的 wire 往返、Debug/递归日志脱敏、不可重绑和停止/失败/重新运行后的失效均有测试覆盖；该协议已进入 `62bed0c071a1d167` 镜像，启动响应保密与停止后的失效已有 Web HTTP 证据；真实确认保存仍未验收。
- 活动运行写入策略的 USB wire 矩阵和蜂鸣器只读例外测试通过；devd 响应/错误两类帧的回归测试在修复前观察到 502，修复后均为 409。固件 host 677 项库测试和 607 项运行时测试通过，devd 全量检查、Rust structural check、host 与 Xtensa 严格 Clippy 通过。该源码保护已进入 `62bed0c071a1d167` 镜像；匹配镜像的跨客户端写入拒绝仍待实测。
- 当前工作树的完整功能 Xtensa release 可编译；host 固件测试分别为 672 项与 604 项通过。目标专属分支的构造字段与 defmt 参数类型必须通过显式 `--target xtensa-esp32s3-none-elf` 构建检查，host 构建不提供此覆盖。
- 完整功能 Xtensa release 与无网络检查产物均通过栈门禁。完整功能镜像的嵌套 PD 中断余量为 98,460B（下限 65,536B），USB 控制链余量为 67,020B、LAN 控制链余量为 51,004B（下限均为 32,768B）。EEPROM 读取域记录以堆分配数组传递，写入域记录按需逐份构造，避免大枚举数组的栈副本；域选择、写入顺序、readback 和提交标记语义保持不变。
- Rust structural check（含 14 项 checker fixtures）、host firmware Clippy 与 Xtensa Clippy 已通过。Web 相关单测 61 项、Biome 全量检查、typecheck 和 production build，以及 devd 测试已通过；这些是源码和构建证据，不替代当前设备镜像的 HTTP 完整调优验收。Web 读取 effect 显式依赖读取状态，操作组使用具名 fieldset 并重置浏览器默认边框、间距和最小宽度。
- 离线 factory-app 镜像大小检查统一通过 `just firmware-image-check` 执行，完整功能镜像为 2,079,184B / 2,097,152B，已通过分区大小检查。当前 Candidate 的真机完整调优、浏览器证据及 PR current-head CI/review 仍未收敛。
- `web/e2e/firmware-tuning-polling.spec.ts` 通过真实 React 组件与浏览器 IndexedDB 验证：成功读取活动运行后，三次失败停读并保留快照；手动重试只开启一轮有限读取；停止按钮保持可用并将对应 runId 的 `cancel` 传入 transport 回调。fixture 不连接 devd、设备或外部服务，不能替代真机验收。停止按钮在读取重新确认期间被错误禁用的问题已由该测试复现并验证修复。
- Web 经 devd LAN HTTP 的运行 `web-pps3a-9711ee99-0fb9-4a43-9a62-440ec540a055` 完整收取 809 条事件，游标连续推进，未再次耗尽事件缓冲；该运行仍失败，首个 60°C 试验在健康 PPS 调压附近记录 5 帧逻辑 100% / 实际 0%，被物理全输出资格拒绝。高优先级许可遗漏 `AutomaticThermal` 已由回归测试复现，修复后 host 测试通过；匹配固件复测仍失败。后续定点诊断确认零输出由后端主动关断和 settle 窗口产生，期间 PD permit 有效、供电响应故障未锁存；100mV 上限往返的回归测试复现了该问题。电压余量与安全合同保持的修复通过后，仍须取得新的完整真机调优证据。
- Web HTTP 运行 `web-pps3a-5a44b5b5-5bf9-4b64-ae51-c30250e4412a` 在约 367.3 秒由 `host_cancel` 结束，事件范围为 0..1313；该运行未完成八目标资格。其烧录 ELF 在最后的恢复逻辑编辑前已生成，不能证明当前工作树的固件修复有效，也不能作为当前 Candidate 的通过证据。必须在目标产物安全检查通过后重新烧录并取得匹配产物身份的完整验证。
- 固件 `d746a1277a1e404b138fab91c6fa5962a1cade99` 的 Web HTTP 运行 `web-pps3a-cb6d1e73-0f5c-44ca-b543-5b4cbcf7b404` 收取 989 条事件，在 271.4 秒由页面停止；预热连续 118 个样本未出现零输出，运行进入后续候选试验，但 HTTP 读取反复失败，未完成整轮调优。相同镜像的 20 次只读 health 检查出现连接拒绝（19 次成功、1 次失败）；socket 回收修复仍需匹配镜像烧录后的同路径复测。
- 固件 `0dc39f96cd321a73b2c0771d37801ae82287690b` 已经 devd/CLI 烧录到授权端口，ELF SHA-256 为 `b726bda6647d77673c6bfd8c55a9359a37854b6bfd6da6b7df06359a0a71d8ca`；HTTP identity 确认匹配版本，旧凭据保持有效。准备阶段一个 PPS 档位余量的回归测试覆盖已确认、未确认、低于工作点和越过上限的合同。Web 经 devd LAN HTTP 发起运行 `web-pps3a-58ea705a-73cb-4101-91ae-b0886add96ee`，已进入预热和后续候选试验，完整运行结果待观察；高频 health 额外负载的 100 次读取仍有 3 次连接拒绝，不能宣告 HTTP 并发验收通过。烧录按授权跳过 EEPROM 备份，未生成备份归档，EEPROM 健康状态未知。
- devd `9b4540d1` 的 Web HTTP 运行 `web-pps3a-469a34d8-2aee-4724-bad5-dcbac09d45b9` 在约 556 秒后由页面停止，期间同一页面额外发出的八个并发状态读取全部返回 200，调优读取未停轮询。60°C 试验仍被物理全输出资格拒绝，220°C 试验发生预热超时并按既有安全规则从较暖起点重试；这不是完整调优通过证据。
- 固件 `a271705e795bc062cfeba1bc0eae913c259defbc` 已通过 672 项库测试、605 项运行时测试、Xtensa Clippy、2,079,200B 镜像和栈门禁，烧录后 HTTP identity 核验匹配，ELF SHA-256 为 `32d56a52845e1b0b466879a355091465633637797c8d7838aad434ee653acf44`。Web HTTP 运行 `web-pps3a-59426f47-b9d6-45f8-9a16-5dd56605825b` 在 60°C 第三项试验的 Approach → Warmup 转换时再次记录两帧实际零输出（序号 1035、1036，286.318s / 286.439s）；当时 PPS 10.8V 已确认、安全上限 10.9V，温度为 54.72°C / 54.87°C。该试验被全输出资格拒绝，之后 220°C 预热仍未达到目标，运行由页面停止并确认已取消；不构成整轮调优通过证据。

- 已有 LAN client 测试覆盖 read-only thermal tuning GET 不携带 lease，以及基础传输错误映射。
- 已有 firmware route 和 Web Serial thermal tuning 测试覆盖命令解析与状态读取基础路径。
- 本主题的完整验证必须补充 `VER-LTR-001` 至 `VER-LTR-019` 对应的 fixture、组件测试、固件契约检查和本地浏览器验证；不得使用线上设备或线上版本代替本地验证。

- 调优采样在请求非零而实际输出为零时保留 `output_paused` 原因，包括 PWM 关断来源、PD permit、供电响应锁存、settle 窗口以及最后一次 PPS 停热分支和缓存/目标电压。该诊断不修改输出授权、安全门禁或试验资格；`p`、`l`、`s` 依次为 PD 许可、供电锁存和等待窗口状态，`active`、`cap` 保存停热决策时的合同电压与上限；`flags` 的低三位依次代表安全恢复、合同纠正和范围/大步长切换；最后一次停热上下文不等同于当前故障归因，须结合时间与状态核验。

- 固件 `2661678676f9162265fa8cab70621148c8d0c54b` 的 Web HTTP 运行 `web-pps3a-3802e0d8-9b3a-4912-adf9-427481b456bb` 在 42.816 秒记录 `output_paused:command permit=1 latch=0 settle=1 flags=2 cached=10700 next=10800`，证明归零来自 PPS 合同纠正分支而非 PD 许可撤销。Web 停止未在 15 秒内确认；通过同一授权设备的 USB/devd 安全停止接口返回 `armed=false`、`canceled`。该轮未完成，不能用于验收通过。

- 固件 `b813e06b2d89c5a3` 的完整功能镜像经 `just firmware-image-check` 确认为 `2,082,096B / 2,097,152B`，通过栈门禁并烧录至主人授权的精确端口；ELF SHA-256 为 `610184f97d3c42204f6a0de6889204dc281c8dc951e6ed9d2bf6e84e8fe73212`。本地 Web 页面通过 devd LAN bridge 连续两次发送空 lease 的 HTTP `cancel`，均返回 200、`armed=false`、`phase=idle`，旧凭据仍有效。该结果只验证幂等停止，不是完整调优通过证据。烧录按授权跳过 EEPROM 备份，未生成备份归档，EEPROM 健康状态未知。
- HTTP 运行 `web-pps3a-1a8acc31-2c5f-4f39-a57e-e6abebf8ea41` 在 7.443 秒捕获合同 10.0V / 上限 9.9V 导致的输出归零；47.059 秒与 48.309 秒也出现合同纠正停热。436.108 秒时仍处于 220°C 预热、实测 108.86°C，后由页面停止并确认已取消，未完成整轮。工作余量回归测试重放上限上升后回落的序列，修复前明确失败于 `Warmup: 10000 -> 10000 at 10000`；必须取得持续保留余量修复的匹配镜像真机证据。
- 固件 `014d8eb210bfc3f2` 的 Web 经 devd LAN HTTP 运行 `web-pps3a-c8c8884d-95f5-4544-afd7-61a8c9d9c0c9` 在 754 秒前收取 2,664 条事件，预热请求全输出期间未见实际归零；60°C 候选仍未通过接近阶段时间预算。随后连续 HTTP 读取失败耗尽页面重试预算，设备事件缓冲在约 822.9 秒耗尽并进入 `failed / armed=false / review_incomplete`，不是整轮通过。诊断读取确认原 runId 仍存在；补收保留事件后记录 2,919 条事件，其中 2918 因缓冲已满未保留，2919 是诊断中错误超前确认游标产生的 `event_ack_skipped_unconfirmed`，不能当作原始失败原因。停机后 20 次公开 health 读取全部成功，尚不足以排除运行期间的瞬态 HTTP 失败。

## Remaining Gaps

- USB 辅助认证保留设备公布的 mDNS 名称，优先对 USB 网络快照中的私有 IPv4 执行匿名 health 身份核验，再以 bearer 读取 HTTP identity，匹配 USB deviceId 后才保存 locator。真机测量中 `.local` 的 health/status 请求分别耗时约 5.05/5.14 秒，IP 请求约 0.04/0.17 秒；原 GET 6 秒预算可成功，POST 3 秒预算则超时，导致页面可读温度却无法开始。认证后经 Web HTTP 发起 `web-pps3a-9cc7d46b-ca12-4548-a987-6ceaed30772d` 已进入运行，完整调优结果仍待核验。匿名身份核验测试覆盖匹配、错设备和不发送 Authorization；devd 测试 584 项通过、4 项忽略，严格 Clippy 通过。

- 固件 `1b09a174` 的完整功能镜像为 2,085,296 / 2,097,152B，ELF SHA-256 为 `fac873a463f9e3be37349468a27b6f6446508b714d72cfb153625cb964fef402`；授权烧录完成，HTTP identity 读回 `0.30.10-dev.1b09a17`。烧录按授权跳过 EEPROM 备份，未生成归档且健康状态未知。本地唯一 Web 页面经 devd LAN HTTP 发起 `web-pps3a-3263afdf-6cf1-4e63-96a5-9b63469544c7`，818.154 秒连续收取 sequence 0–2873 共 2,874 条事件。60°C trial 4 通过 scout，但独立 hold_confirm 未通过；trial 6 冷却期间 PD 服务以 available=true、discovering、无 fault/contract/ticket 状态触发 `pd_contract_lost`，整轮失败、heater off、未保存。随后 HTTP status 显示 PPS 12V/3A 已恢复，最后协议故障为 `pd_fusb_pending_request_timeout`，设备未重启。该结果证明这一轮 HTTP 追踪完整，不证明调优通过或请求超时原因已解决。
- 调优恢复状态回归使用生产 `PowerState::from_observation` 构造真实重新发现快照；无故障的 Discovering 与既有可恢复故障使用相同的 30 秒有界等待，等待期间不授予加热许可、不记录资格样本。明确断开、服务不可用和超过恢复期限仍失败。
- 固件 `9d2c68ab` 的完整功能镜像经 `just firmware-image-check` 确认为 2,085,392 / 2,097,152B，ELF SHA-256 为 `150cf79462ceab3a2cda8e42ddcbed2ea8e7d58f8dd1c52865cd2cb77aa053dc`。授权端口烧录完成，HTTP health 读回 `0.30.10-dev.9d2c68a`；跳过 EEPROM 备份且健康状态未知。本地唯一 Web 页面经 devd LAN HTTP 发起 `web-pps3a-4865b584-e567-45f4-8911-4901de6af48c`，1,743.395 秒连续收取 sequence 0–6197 共 6,198 条事件，未因 PD 合同恢复中断、未耗尽 HTTP 重试预算。60°C 十次试验均未通过接近时限；220°C 三次预热均以实际和请求全输出运行，但末温分别约 100.78°C、108.20°C、110.15°C，180 秒阶段超时。最终其余区间因无合格边界剪枝，整轮失败；HTTP status 确认 heaterEnabled=false、实际与请求输出均为零、固定 5V/1A，未保存。该证据不证明未出现的 PD 重新发现路径被实际覆盖，也不证明完整调优通过。
- 纯延迟供热偏差原先取窗口首尾温差，会将测温扰动放大成错误功率。固定 60°C、标定环境 30°C、真实环境 25°C 的回归中，额外散热应约为 1,055mW，但 ±0.75°C 交替扰动导致旧估算为额外输入 9,189mW。`EnergySlope` 以在线最小二乘拟合累计能量偏差与有效观测时间，截距吸收首个温度锚点误差；只保留窗口内统计量，PPS 重锚不跨跳变积分，过期和恢复仍清除校正。原冷环境闭环回归失败、无扰动对照通过，修正后闭环完成 scout 与独立确认；保持原资格时限、3°C 指标和不保存语义。该模拟不经过实机 ADC/PPS 守卫，也不覆盖实机 220°C 预热超时，修正的真机复测仍未完成。

- 对已实现的身份存储、locator 迁移和 401 恢复取得完整跨 adapter fixture 证据。
- 完成 capability 与稳定 unsupported 契约的跨 adapter 验收。
- 完成独立 tuning read state 与顶部连接状态的页面级经验验收。
- 完成端点级错误的 adapter parity 验收，并统一 devd 传输失败的中文提示。
- 完善 owner proof、跨客户端写入禁用和本地浏览器验收证据。
- 为 status 成功、tuning 失败的组合场景补充本地 fixture 和 UI 验收。
- 为 `pps3a` 与 `pps5a` 补充独立工厂数据审计记录和功率档位选择门禁测试。
- 在 firmware、devd、Web Serial 和 LAN adapter 中统一实现 `thermal_parameters_unavailable`、候选预览、确认保存、回滚和跨客户端停止语义。

## Related Changes

- [ADR 0027: Embedded factory thermal baselines before tuning](../../adr/0027-embedded-factory-thermal-baselines.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
