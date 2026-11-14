# Flux Purr

本词汇表约定 Flux Purr 产品评审与演示中的领域用语。

## 发布与版本

**Manual Release（手动发布）**：由 owner 确认 `release_target` 后通过 `Release Product` manual dispatch 启动的产品发布流程；它不允许 caller 直接指定 canonical version，也不能绕过 PR、review、required checks 或 `main` protection。

**Release Intent（发布意图）**：owner 确认的单一目标 `patch`、`minor`、`major` 或 `rc`。它不是最终版本号，workflow 根据 trusted baseline 独立解析 canonical version。

**Trusted Stable Baseline（可信稳定基线）**：当前 release source 可达的最高稳定 final tag；它用于解析下一个 stable 或 RC 版本，不由 PR labels、merge 顺序或 package metadata 推导。

**Release Identity（发布身份）**：绑定 source SHA、baseline、intent、canonical version、tag reservation、artifact digests、GitHub Release、部署结果和 recovery target 的不可歧义记录。

**Tag Reservation（标签预留）**：在构建、创建 Release 或部署前，将 `v<version>` 绑定到 exact source SHA 的不可变占位步骤；collision 必须 fail closed。

**Same-identity Recovery（同身份恢复）**：只复用已经存在且精确指向当前 source SHA 的 release tag，补齐失败的 Release/Deploy/Notify 步骤；不重新计算版本、递增 RC、移动 tag 或创建 successor。

**Documentation-only Change（纯文档变更）**：变更路径全部属于 `docs/**`、`README*`、`CHANGELOG*`、Markdown、纯文本或文档图片资源的提交集合。只要混入其它产品路径，就按产品变更处理。

**Version Suggestion（版本建议）**：Agent 根据当前 baseline 与变更范围向 owner 提出的 intent/预期版本说明；它不写入仓库、不持久化、不替代 owner 确认，也不作为 workflow 的 canonical version 输入。

## Language

**正式控制台（Live Console）**：用于查看和操作已授权真实 Flux Purr 设备的产品界面，其设备状态与控制结果来自真实设备。
_Avoid_：可切换演示的正式控制台

**演示控制台（Demo Console）**：用于体验模拟 Flux Purr 设备行为的产品界面，沿用正式控制台的产品交互，设备操作均为模拟。
_Avoid_：组件画廊、真实设备控制台

**演示场景（Demo Scene）**：用于体验 Flux Purr 控制台的一组设备、运行态、故障、能力与数据初始条件；分享场景时还原这些初始条件，不包含后续操作结果。
_Avoid_：当前完整状态、操作回放

**模拟操作结果（Simulated Mutation）**：在演示场景中通过热控、风扇、网络或校准操作产生的临时设备状态变化，不属于该场景的初始条件。
_Avoid_：演示场景、日志消息

**正常自动加热（Normal Automatic Heating）**：操作者设置目标温度后，由设备自动调节加热输出以达到并维持目标温度的产品工作模式。

**标定加热（Calibration Heating）**：为取得温度、电压、整机供电边界或热模型的标定观测而进行的受控加热过程。

**手动 PPS 调试（Manual PPS Debugging）**：操作者显式指定 PPS 电压与请求电流的调试模式，可与加热板通电同时存在。

**理论加热板电阻（Theoretical Heater Resistance）**：根据加热板设计电气参数推算的负载电阻，与通过标定观测得到的电阻相区分。

**整机供电边界（Whole-device Supply Boundary）**：在特定 WiFi 硬件状态和 PPS 源能力下，按温度与合同电流记录的电压跟随/压低边界；包含整机负载，不代表独立加热板电阻或 MCU 功耗。

**电源电流能力（Source Current Capability）**：电源在特定输出电压及对应供电档位下允许负载使用的电流上限，与负载实际取用的电流相区分。

**合同电流限值（Contract Current Limit）**：设备请求并由电源确认的电流限制设定，不是负载实际电流读数。正常自动加热不从该值扣除固定辅助电流。

**自动加热安全电压上限（Automatic Heating Voltage Ceiling）**：由适用的整机边界或明确标识的预设模型兜底，并受当前电源/合同/硬件能力约束的最高请求电压。

**安全供电工作点（Safe Supply Operating Point）**：一组满足电源输出范围、设备供电条件与适用整机边界的供电电压和合同电流。

**RTD 理论温度（Theoretical RTD Temperature）**：由 RTD 原始读数、对应的分压电路标称参数和 PT1000 理论关系换算出的温度，不包含用户校准修正，也不等同于独立温度计实测值。

**温度—申请电压图（Temperature–Requested Voltage Plot）**：按每条供电观测的 RTD 理论温度与申请电压展示原始散点，并提供同合同电流内审查拟合的图表；合同电流是该观测的适用条件，不是负载实测电流。

**插值结果（Interpolated Result）**：在图表有效 X 轴范围内，根据拟合曲线或温度轨迹计算出的当前 X 位置 Y 值；不是新的原始观测，也不应写回设备。

**离散供电观测（Discrete Supply Observation）**：供电观测表中的一条原始采样记录，可由图表中的离散点选中。

**选中观测（Selected Observation）**：当前被图表或供电观测表选中的离散供电观测；图表点和表格行保持同步高亮。

**凭据拒绝（Credential Rejection）**：远端明确确认提交的身份凭据未被接受的失败结果；仅凭模糊的客户端文案不能得出该结论。

**Endpoint 不可用（Endpoint Unavailable）**：请求在部署开始前因服务端 5xx、连接超时、连接重置、连接拒绝或 DNS 等通信问题无法完成的状态。

**部署状态不确定（Deployment Ambiguous）**：上传或远端接受部署请求后，客户端无法确认远端最终状态的结果；此状态要求停止自动重试。

**认证或 Endpoint 未能判定（Auth or Endpoint Ambiguous）**：客户端输出不足以区分凭据拒绝和 Endpoint 不可用的失败结果；该术语不表示凭据已经失效。

**设备身份（Device Identity）**：由设备发布、用于稳定识别同一物理设备的 `deviceId`；它独立于 DHCP 地址、浏览器连接方式和当前租约。
_Avoid_：设备地址、连接地址、IP 身份

**网络定位地址（Network Locator）**：浏览器当前用于访问设备的 `baseUrl`；它只描述请求发往哪里，不代表设备身份或配对关系。
_Avoid_：设备身份、配对地址

**设备连接（Device Connection）**：客户端已经验证目标设备身份，并能读取基础身份、网络或运行态事实的状态；单个功能请求失败不自动推翻该状态。
_Avoid_：所有功能都可用、调优已就绪

**调优读取状态（Tuning Read State）**：固件热控调优页面最近一次读取操作的结果状态，包括读取中、成功、未支持、未授权、传输失败和协议失败。
_Avoid_：设备连接状态、固件版本状态

**控制租约（Control Lease）**：允许客户端修改设备控制输入的短生命周期授权；只读状态读取不需要控制租约。
_Avoid_：连接凭据、配对凭据

**配对凭据（Pairing Credential）**：设备通过物理配对确认后签发的 bearer 访问凭据，按设备身份保存并在设备明确拒绝时失效。
_Avoid_：配对码、IP 密码

**能力声明（Capability Declaration）**：设备身份中对当前固件实际提供的控制面接口或功能的稳定声明；客户端只能依据该声明或明确协议响应判断功能是否提供。
_Avoid_：前端猜测、按钮可见性
