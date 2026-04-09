# Flux Purr 双版本风扇 PCB 方案（5V / 12V）（#v5k2p）

## 状态

- Status: 已完成
- Created: 2026-04-10
- Last: 2026-04-10

## 背景 / 问题陈述

- 当前仓库只冻结了一套 `TPS62933DRLR` 可调风扇电源轨，输出目标为 `3.0 V ~ 5.0 V`，默认对应 `5V` 风扇。
- 主人要求在不改动 MCU / firmware 对外契约的前提下，同时维护 `5V` 与 `12V` 两种风扇 PCB 变体。
- 共享 firmware / API 不应该把 `fan_pwm_permille` 绑定成“可推导风扇电压”的字段；未来风扇调节应面向温度闭环和归一化执行量，而不是面向电压曲线。

## 目标 / 非目标

### Goals

- 维护两套 sibling PCB 变体：`fan-5v` 与 `fan-12v`。
- 冻结共享 GPIO / firmware 契约：`GPIO35 = FAN_EN`、`GPIO36 = FAN_VSET_PWM`、`GPIO34 = FAN_TACH`，以及 `fan_enabled` / `fan_pwm_permille` 状态字段不变。
- 保留当前 `5V` 可调风扇轨方案；新增 `12V` 可调风扇轨方案，不改成 4 线 PWM 拓扑。
- 冻结 `12V` 版的 `RFBT=124kΩ`、输出电容耐压/封装要求、丝印告警与制造输出命名。
- 订正 firmware / API 口径：`fan_pwm_permille` 只表示 normalized actuator，不表示风扇电压，shared firmware 也不区分 `fan-5v` / `fan-12v`。

### Non-goals

- 不把两版压成“单 PCB + 装配差异”的统一板型。
- 不新增 Web / HTTP API 字段。
- 不在本轮实现 tach 闭环调速、失速恢复或 4 线 PWM 风扇支持。
- 不在本仓库伪造未生成的 Gerber 实体文件；仓库内只冻结变体 manifest、命名与 BOM 覆盖规则。

## 范围（Scope）

### In scope

- `docs/specs/v5k2p-dual-fan-pcb-variants/SPEC.md` 与 `docs/specs/README.md`。
- `docs/hardware/tps62933-dual-rail-power-design.md`、`docs/hardware/s3-frontpanel-baseline.md`、`docs/hardware/fan-pcb-variants.md`。
- `docs/hardware/variants/fan-5v/**` 与 `docs/hardware/variants/fan-12v/**` 的变体 manifest / BOM 覆盖文件。
- `firmware/src/lib.rs`、`firmware/src/bin/esp32s3_fan_cycle.rs` 与 `firmware/README.md` 的共享风扇控制契约订正。
- `README.md` 与 `docs/interfaces/http-api.md` 的同步说明。

### Out of scope

- 真实 PCB CAD 源（EasyEDA / KiCad）重布线与导出。
- 实机波形、纹波、热与起转可靠性 bench 测试。
- 新增风扇接口线序或更换连接器家族。
- 在 shared firmware contract 中引入板型选择器或电压估算模型。

## 需求（Requirements）

### MUST

- 两个变体继续共用 `TPS62933DRLR + GPIO36 PWM -> RC -> FB 注入 + GPIO35 EN` 控制拓扑。
- `fan-5v` 继续冻结为：
  - `RFBB = 10 kΩ`
  - `RFBT = 47 kΩ`
  - `RINJ = 75 kΩ`
  - `RPWM = 10 kΩ`
  - `CPWM = 1 uF`
  - `REN_PD = 100 kΩ`
  - `RSER_EN = 2.2 kΩ`
  - 目标 `FAN_VCC ≈ 3.0 V ~ 5.06 V`
- `fan-12v` 继续共用上述拓扑，但必须把 `RFBT` 改为 `124 kΩ 1%`，其余控制网络不变。
- `fan-12v` 的目标输出范围必须约为 `6.6 V ~ 12.0 V`。
- `fan-12v` 的 `FAN_VCC` 输出电容必须升级为 `>=25 V` 额定且 `1206` 或更大，并保持最少 `2 x 22 uF` 主输出电容 + `100 nF` 本地去耦要求。
- 板丝印必须明确区分：`5V FAN ONLY` 与 `12V FAN ONLY`。
- 制造输出命名必须分离：`fan-5v` 与 `fan-12v` 不能共享导出文件名。
- firmware / API 继续只暴露 `fan_enabled` 与 `fan_pwm_permille`；shared firmware 不得编码 `permille -> mV` 映射，也不得据此区分 `5V` / `12V` 版本。
- `fan_pwm_permille` 在 shared firmware contract 中必须被定义为 normalized actuator setpoint，便于后续温度闭环直接驱动。
- 任意板级电压曲线、低速起转特性、启动 boost 或其他 variant-specific tuning 都应留在硬件文档 / bring-up 语境，而不是 shared firmware model。

### SHOULD

- `fan-5v` 继续作为 archived base netlist 的直接代表板型。
- `fan-12v` 通过 overlay manifest / BOM 覆盖规则与 archived base netlist 关联，而不是重新发明另一套 GPIO / 控制契约。
- 变体 manifest 以机器可读格式保存，便于后续 CAD / 生产脚本消费。

### COULD

- 后续新增温度闭环时，直接使用 `fan_pwm_permille` 作为归一化执行量输出，而无需重新定义面向电压的 shared contract。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `fan-5v` 与 `fan-12v` 共享 `GPIO35/36/34` 硬件所有权与状态字段语义。
- archived base netlist 继续保留在 `docs/hardware/netlists/main-controller-board.enet`，并视为 `fan-5v` 基线。
- `fan-12v` 通过单独 manifest / BOM 覆盖说明：`RFBT` 替换为 `124 kΩ`，`FAN_VCC` 输出电容升级到 `>=25 V / 1206+`。
- firmware core 只处理 `fan_enabled` + `fan_pwm_permille` 这组 normalized control contract，不维护 shared 5V / 12V 电压 profile。
- `fan_pwm_permille` 的含义保持为“归一化设定值 / actuator effort”，而不是“所有板型都等价的固定毫伏数”。
- 后续若接入温度闭环，闭环输出仍应落在同一组 normalized fan contract 上。

### Edge cases / errors

- 若调用方把 `fan_pwm_permille` 解释成单一固定电压曲线，文档必须显式指出该解释错误。
- `fan-12v` 输出电容不得继续沿用当前 `C0603 6.3V/10V` 组合，否则应视为配置错误。
- 若某个具体 `12V` 风扇在低速下需要额外起转策略，该策略必须视为 board-specific bring-up tuning，而不是 shared firmware contract 的一部分。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Device status fan fields | Rust / HTTP model | external + internal | Clarify | `../../interfaces/http-api.md` | firmware | firmware / web / bring-up docs | 字段不变，明确不可由此推导风扇电压或 PCB 版本 |
| Fan PCB variant manifests | JSON / CSV | hardware-doc internal | New | `../../hardware/fan-pcb-variants.md` | hardware docs | future CAD / manufacturing flow | 冻结 BOM 覆盖与导出命名；硬件电压曲线留在此层 |

## 验收标准（Acceptance Criteria）

- Given `fan-5v` 变体，When 检查 hardware docs 与 manifest，Then 风扇轨仍为 `3.0 V ~ 5.06 V`，且 `RFBT = 47 kΩ`。
- Given `fan-12v` 变体，When 检查 manifest，Then 风扇轨为约 `6.6 V ~ 12.0 V`，且 `RFBT = 124 kΩ 1%`。
- Given `fan-12v` 变体，When 检查 BOM 覆盖规则，Then `FAN_VCC` 主输出电容均为 `>=25 V` 且 footprint 为 `1206` 或更大。
- Given shared firmware core，When 检查公开 fan contract，Then 只存在 `fan_enabled` 与 `fan_pwm_permille` 这组 normalized setpoint 语义，而不存在 shared 5V / 12V 电压估算模型。
- Given HTTP contract，When 检查 `fanPwmPermille` 字段说明，Then 文档明确该值是 normalized actuator，而不是跨板型固定电压。

## 实现前置条件（Definition of Ready / Preconditions）

- 主人已明确要求同时支持 `5V` 与 `12V` 风扇 PCB 变体。
- `12V` 版继续采用可调风扇电压轨，而不是 4 线 PWM 风扇拓扑。
- 共享 GPIO / firmware contract 保持不变，且 firmware 不区分板型电压版本。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test --manifest-path firmware/Cargo.toml`
- Build checks: `bash scripts/check-firmware-build.sh`
- Hardware docs review: manifest / BOM / control-law cross-check

### Quality checks

- `bash scripts/check-firmware-fmt.sh`
- `bash scripts/check-firmware-clippy.sh`
- `bash scripts/check-firmware-build.sh`

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/hardware/tps62933-dual-rail-power-design.md`
- `docs/hardware/s3-frontpanel-baseline.md`
- `docs/hardware/fan-pcb-variants.md`
- `docs/interfaces/http-api.md`
- `firmware/README.md`
- `README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增双版本风扇 PCB spec 与索引
- [x] M2: 冻结 `fan-5v` / `fan-12v` 变体 manifest、BOM 覆盖与制造输出命名
- [x] M3: 订正 shared firmware / API fan contract，使其保持电压不可知并同步对外文档说明

## 方案概述（Approach, high-level）

- 把 archived base netlist 继续作为 `fan-5v` 的直接代表，避免无谓复制大文件。
- 使用 `fan-12v` overlay manifest + BOM 覆盖规则描述最小必要差异：`RFBT`、输出电容、丝印与制造输出命名。
- 保留硬件层的输出范围与控制曲线说明，但不把这些曲线提升成 shared firmware model。
- 在硬件 / firmware / API 文档里统一声明：`fan_pwm_permille` 是 normalized setpoint，shared firmware 不根据它推导风扇电压。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`TPS62933DRLR + 3.3 uH` 在 `12V / 0.5A` 风扇上的热、纹波与冷启动表现仍需实板验证。
- 风险：当前仓库只保存 archived netlist，没有 live CAD 源，因此制造输出命名与 BOM 覆盖能冻结，但真实 Gerber/坐标文件仍需在 CAD 环境中生成。
- 风险：若实际 `12V` 风扇启动行为高于预期，可能仍需要 board-specific bring-up tuning，但不应反向污染 shared firmware contract。
- 假设：当前 `main-controller-board.enet` 继续作为 `fan-5v` archived baseline 使用。
- 假设：`fan-12v` 版的本地 `100 nF` 去耦会在 live CAD 源里新增/分配实际 designator。

## 参考（References）

- [../../hardware/tps62933-dual-rail-power-design.md](../../hardware/tps62933-dual-rail-power-design.md)
- [../../hardware/s3-frontpanel-baseline.md](../../hardware/s3-frontpanel-baseline.md)
- [../../hardware/fan-pcb-variants.md](../../hardware/fan-pcb-variants.md)
- [../../interfaces/http-api.md](../../interfaces/http-api.md)
