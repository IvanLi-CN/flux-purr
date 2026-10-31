# Flux Purr

本词汇表约定 Flux Purr 产品评审与演示中的领域用语。

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
