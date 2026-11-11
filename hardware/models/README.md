# Mechanical Models

本目录保存 Flux Purr 外壳和结构件的 CAD 源文件。按产品尺寸划分工程目录，通用结构件直接放在本目录下。

| 目录或文件 | 内容 |
| --- | --- |
| [`5p6cm/`](5p6cm/) | 5.6 cm 加热板外壳工程，包括底壳、内框和 FreeCAD 工程文件。 |
| [`7cm/`](7cm/) | 7 cm 加热板外壳工程，包括底壳、内框和 FreeCAD 工程文件。 |
| [`flux-purr-Joystick.step`](flux-purr-Joystick.step) | 通用摇杆帽 STEP 模型。 |

STEP 文件用于跨 CAD 工具交换，FCStd 文件是 FreeCAD 原生工程文件。模型尺寸、适配的加热板和硬件关系见 [`docs/hardware/`](../../docs/hardware/) 下的结构设计文档。

## 加工清单与数量

以下数量是组装一台设备的默认数量；同一尺寸的设备每增加一台，按对应版本整套数量增加。3D 打印件的打印精度不得低于 `1 mm`，所选材料的耐热温度不得低于 `56 °C`。

### 5.6 cm 版本

| 加工件 | 文件 | 默认数量 | 加工方式 |
| --- | --- | ---: | --- |
| 底版 | [`flux-purr-5p6-Bottom.step`](5p6cm/flux-purr-5p6-Bottom.step) | 1 | 3D 打印或双面 CNC |
| 内框 | [`flux-purr-5p6-Iframe.step`](5p6cm/flux-purr-5p6-Iframe.step) | 1 | 3D 打印 |
| 摇杆帽 | [`flux-purr-Joystick.step`](flux-purr-Joystick.step) | 1 | 3D 打印 |

### 7 cm 版本

| 加工件 | 文件 | 默认数量 | 加工方式 |
| --- | --- | ---: | --- |
| 底版 | [`flux-purr-7-Bottom.step`](7cm/flux-purr-7-Bottom.step) | 1 | 3D 打印或双面 CNC |
| 内框 | [`flux-purr-7-Iframe.step`](7cm/flux-purr-7-Iframe.step) | 1 | 3D 打印 |
| 摇杆帽 | [`flux-purr-Joystick.step`](flux-purr-Joystick.step) | 1 | 3D 打印 |

底版模型已按双面 CNC 加工场景保留完整几何，可作为双面装夹和分步加工的源模型。实际 CNC 加工前仍需由 CAM 工程确认基准、装夹、刀路和余量；STEP 文件本身不包含具体机床刀路。
