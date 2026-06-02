# Flux Purr 7.0cm 3.2Ω 加热板变体（#kht7p）

## 状态

- Status: 已完成
- Created: 2026-05-31
- Last: 2026-05-31

## 背景 / 问题陈述

- `heater-5p6-3p2` 已冻结为 `56 mm x 56 mm`、`R20 = 3.2 ohm` 的正式加热板版本。
- `70 mm x 70 mm`、`R20 ~= 3.2 ohm` 加热板作为正式硬件变体纳入项目文档和制造资产。
- 新板尺寸更大，但仍属于同一 `3.2 ohm` 冷态负载方案；固件不按物理尺寸区分这一类加热板。

## 目标 / 非目标

### Goals

- 正式硬件 profile：`heater-7p0-3p2`。
- 保存已检查的 `70 mm x 70 mm` 生产 Gerber 包与 SHA-256。
- 在 heater plate family 文档中登记新变体。
- 记录新变体的 Gerber 解析尺寸、电阻估算、制造注意事项与热性能设计目标。

### Non-goals

- 不修改 firmware、Web UI、HTTP API 或运行时字段。
- 不新增按加热板尺寸区分的固件 profile 选择机制。
- 不把 `220 C hold at 20 C ambient` 写成已验证能力；该能力需要首件实测确认。

## 范围

### In scope

- `docs/hardware/heater-plate-design.md`
- `docs/hardware/heater-plates/heater-7p0-3p2.md`
- `docs/hardware/gerbers/heater-plate-7p0cm-3p2ohm/**`
- `docs/specs/kht7p-heater-7p0-3p2-variant/SPEC.md`
- `docs/specs/README.md`

### Out of scope

- 固件 PID、PPS/AVS 调压逻辑、前面板显示和 Web 控制台。
- 实物打样、温度均匀性测量、保温功率验证。
- CAD 源文件托管；本仓只保存制造导出包和文档化解析结果。

## 需求

### MUST

- `heater-7p0-3p2` 必须记录为 `70 mm x 70 mm`、`R20 = 3.2 ohm` 变体。
- 接受冷态电阻范围必须沿用 `3.2 ohm` 方案：`3.1 ohm <= R20 <= 3.3 ohm`。
- Gerber 包必须保存到 `docs/hardware/gerbers/heater-plate-7p0cm-3p2ohm/flux-purr-heater-plate-7p0cm-3p2ohm-gerbers.zip`。
- 文档必须记录 Gerber 包 SHA-256：`c1c3742df95d7034359f85b2adfa2905967c44bfac6cd6a3168c0da025c3d026`。
- heater family 支持矩阵必须包含 `heater-7p0-3p2`。
- 文档必须说明 `heater-5p6-3p2` 与 `heater-7p0-3p2` 共用 firmware-facing `3.2 ohm` load class；固件不得根据物理面积推断加热板版本。
- 热性能必须写成设计目标，不能写成已验证事实。

### SHOULD

- 变体文档应复用现有 heater plate family 的电气模型与制造注意事项。
- Gerber 解析结果应覆盖板框、孔径、线宽、线长、间距、冷态电阻估算和底层铜状态。
- 订单说明应继续强调 `1 oz` 铜；错误选择 `2 oz` 会破坏电阻目标。

## 功能与行为规格

### Core flows

- 硬件成员从 `docs/hardware/heater-plate-design.md` 查看支持版本，能发现 `heater-7p0-3p2`。
- 制造时使用新变体文档中的 Gerber 路径与 SHA-256 校验包。
- Bring-up 时测量实际冷态电阻，若落在 `3.1 ohm <= R20 <= 3.3 ohm`，即可按 `3.2 ohm` 电气负载方案继续验证。
- 固件继续基于测得冷态电阻和现有 `3.2 ohm` 电气假设限制功率，不新增尺寸选择字段。

### Edge cases / errors

- 若制造商使用 `2 oz` 铜或改动线宽/铜厚，则该 Gerber 文档中的 `R20` 估算失效。
- 若实测冷态电阻超出 `3.1 ohm ~ 3.3 ohm`，该板不得直接按 `heater-7p0-3p2` profile 放行。
- 若首件无法在目标结构下保温 `220 C`，应更新热性能说明或另建变体，不得反向修改电气事实。

## 接口契约

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Heater plate profile catalog | Hardware docs | internal | Add `heater-7p0-3p2` | `../../hardware/heater-plate-design.md` | hardware docs | firmware bring-up / manufacturing | 新尺寸，仍为 `3.2 ohm` load class |
| Heater Gerber package | Manufacturing asset | internal | Add package | `../../hardware/heater-plates/heater-7p0-3p2.md` | hardware docs | manufacturing / bring-up | SHA-256 固定 |

## 验收标准

- Given `docs/hardware/heater-plate-design.md`，When 查看 supported versions，Then 表格包含 `heater-7p0-3p2`。
- Given `docs/hardware/heater-plates/heater-7p0-3p2.md`，When 查看 profile，Then 能看到 `70 mm x 70 mm`、`R20 = 3.2 ohm` 与 `3.1 ohm <= R20 <= 3.3 ohm`。
- Given `heater-7p0-3p2` Gerber zip，When 运行 `unzip -t`，Then 压缩包完整无错误。
- Given `heater-7p0-3p2` Gerber zip，When 计算 SHA-256，Then 等于 `c1c3742df95d7034359f85b2adfa2905967c44bfac6cd6a3168c0da025c3d026`。
- Given `heater-7p0-3p2` 变体文档，When 检查热性能表述，Then 只能看到设计目标和实测前置条件，不得宣称已验证保温能力。
- Given 代码变更范围，When 检查 diff，Then 不包含 firmware、Web UI 或 HTTP API 行为改动。

## 实现前置条件

- `heater-7p0-3p2` 是正式硬件变体。
- `heater-7p0-3p2` 属于现有 `3.2 ohm` 冷态负载方案，固件不按尺寸区分。
- Gerber 包已由本地解析确认：主线宽 `0.45 mm`、主线长约 `2904.61 mm`、估算 `R20 ~= 3.19 ohm`。

## 非功能性验收 / 质量门槛

### Testing

- `unzip -t docs/hardware/gerbers/heater-plate-7p0cm-3p2ohm/flux-purr-heater-plate-7p0cm-3p2ohm-gerbers.zip`
- `shasum -a 256 docs/hardware/gerbers/heater-plate-7p0cm-3p2ohm/flux-purr-heater-plate-7p0cm-3p2ohm-gerbers.zip`
- `rg -n "heater-7p0-3p2|heater-plate-7p0cm-3p2ohm|c1c3742d" docs`
- `rg -n 'TO''DO|TB''D|待''补充' docs/hardware/heater-plate-design.md docs/hardware/heater-plates/heater-7p0-3p2.md docs/specs/kht7p-heater-7p0-3p2-variant/SPEC.md`

### UI / Storybook

- N/A

## 文档更新

- `docs/hardware/heater-plate-design.md`
- `docs/hardware/heater-plates/heater-7p0-3p2.md`
- `docs/specs/README.md`
- `docs/specs/kht7p-heater-7p0-3p2-variant/SPEC.md`

## 实现里程碑

- [x] M1: 保存 `heater-7p0-3p2` Gerber 包并记录 SHA-256。
- [x] M2: 完成 `heater-7p0-3p2` 硬件变体文档。
- [x] M3: 更新 heater family 支持矩阵。
- [x] M4: 新增并索引 `#kht7p` 规格。

## 方案概述

- 以 `heater-5p6-3p2` 为电气模型基线，维护更大物理面积的 sibling heater plate。
- 保持 firmware-facing contract 为 `3.2 ohm` cold-resistance class，不新增尺寸字段或固件分支。
- 将 `220 C` 保温写成设计目标，等待首件实测后再提升为已验证结论。

## 风险 / 开放问题 / 假设

- 风险：更大面积会改变升温速度、保温功率和温度均匀性，必须通过首件实测验证。
- 风险：下单铜厚、阻焊和铝基板介质若偏离文档假设，冷态电阻或热表现会偏离。
- 假设：归档的 Gerber zip 是 intended production package。
- 假设：制造按 `1 oz` 单面铝基板执行。

## 参考

- [../../hardware/heater-plate-design.md](../../hardware/heater-plate-design.md)
- [../../hardware/heater-plates/heater-7p0-3p2.md](../../hardware/heater-plates/heater-7p0-3p2.md)
