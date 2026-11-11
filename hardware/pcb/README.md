# PCB Design Assets

本目录保存 Flux Purr 的 PCB 设计工程容器、网表和制造导出物。PCB 工程源文件与 Gerber 包共用本目录，并通过文件名区分板卡和用途。

| 文件 | 内容 |
| --- | --- |
| [`ProPrj_FluxPurr.epro2`](ProPrj_FluxPurr.epro2) | Flux Purr PCB 工程容器。 |
| `*.enet` | 控制板和前面板板卡的网表归档。 |
| `*-schematic.pdf` | 由 EasyEDA Pro 导出的板卡原理图 PDF。 |
| `*-gerbers.zip` | 各 PCB 板卡的 Gerber 制造文件包。 |
| `*-pcb.pdf` | 由 EasyEDA Pro 的 PCB 编辑器导出的 PCB 图纸；文件名包含板卡用途、尺寸或型号。 |

PCB PDF 与工程板卡的对应关系按尺寸分组：

### 7 cm 版本

| PCB PDF | 工程板卡 |
| --- | --- |
| `flux-purr-heater-plate-7p0cm-3p2ohm-pcb.pdf` | `PCB5_2`：加热板 3.2Ω |
| `heater-stack-insulation-board-7p0cm-pcb.pdf` | `PCB4_1`：隔热板 |
| `heater-stack-support-top-board-7p0cm-pcb.pdf` | `PCB10_1`：顶板 |

### 5.6 cm 版本

| PCB PDF | 工程板卡 |
| --- | --- |
| `flux-purr-heater-plate-5p6cm-3p2ohm-pcb.pdf` | `PCB5_1`：加热板 3.2Ω |
| `flux-purr-heater-plate-5p6cm-4p5ohm-pcb.pdf` | `PCB5`：加热板 4.5Ω |
| `heater-stack-insulation-board-bottom-5p6cm-pcb.pdf` | `PCB4_3`：隔热板，下层 |
| `heater-stack-insulation-board-top-5p6cm-pcb.pdf` | `PCB4_2`：隔热板，上层 |
| `heater-stack-insulation-board-5p6cm-pcb.pdf` | `PCB4`：隔热板 |
| `heater-stack-support-top-board-5p6cm-pcb.pdf` | `PCB10`：顶板 |

### 不区分尺寸

| PCB PDF | 工程板卡 |
| --- | --- |
| `main-controller-board-fusb302b-rev-5-2-pcb.pdf` | `PCB6_2`：FUSB302B 控制板 |
| `main-controller-board-ch224q-pcb.pdf` | `PCB6_1`：CH224Q 控制板 |
| `front-panel-board-pcb.pdf` | `PCB11`：前面板 |

原理图 PDF（不区分尺寸）与工程板卡的对应关系：

| 原理图 PDF | 工程板卡 |
| --- | --- |
| `front-panel-board-schematic.pdf` | `PCB11`：前面板；原文件为 `SCH_Schematic8_2026-11-12.pdf` |
| `main-controller-board-fusb302b-rev-5-2-schematic.pdf` | `PCB6_2`：FUSB302B 控制板；原文件为 `SCH_schematic5_2_2026-11-12.pdf` |

Gerber 制造文件与工程板卡的对应关系按尺寸分组：

### 7 cm 版本

| Gerber ZIP | 工程板卡 |
| --- | --- |
| `flux-purr-heater-plate-7p0cm-3p2ohm-gerbers.zip` | `PCB5_2`：加热板 3.2Ω |
| `heater-stack-insulation-board-7p0cm-gerbers.zip` | `PCB4_1`：隔热板 |
| `heater-stack-support-top-board-7p0cm-gerbers.zip` | `PCB10_1`：顶板 |

### 5.6 cm 版本

| Gerber ZIP | 工程板卡 |
| --- | --- |
| `flux-purr-heater-plate-5p6cm-3p2ohm-gerbers.zip` | `PCB5_1`：加热板 3.2Ω |
| `flux-purr-heater-plate-5p6cm-4p5ohm-gerbers.zip` | `PCB5`：加热板 4.5Ω |
| `heater-stack-insulation-board-bottom-5p6cm-gerbers.zip` | `PCB4_3`：隔热板，下层 |
| `heater-stack-insulation-board-top-5p6cm-gerbers.zip` | `PCB4_2`：隔热板，上层 |
| `heater-stack-insulation-board-5p6cm-gerbers.zip` | `PCB4`：隔热板 |
| `heater-stack-support-top-board-5p6cm-gerbers.zip` | `PCB10`：顶板 |

### 不区分尺寸

| Gerber ZIP | 工程板卡 |
| --- | --- |
| `main-controller-board-fusb302b-rev-5-2-gerbers.zip` | `PCB6_2`：FUSB302B 控制板 |
| `main-controller-board-ch224q-gerbers.zip` | `PCB6_1`：CH224Q 控制板 |
| `front-panel-board-gerbers.zip` | `PCB11`：前面板 |

板卡功能、尺寸、功率版本和装配关系见 [`docs/hardware/`](../../docs/hardware/) 下的 PCB 与加热板文档。修改 PCB 工程后，应重新导出受影响的 Gerber 包并同步相关文档。
