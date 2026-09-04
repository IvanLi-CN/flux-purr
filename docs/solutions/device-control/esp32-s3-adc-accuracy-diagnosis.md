---
title: ESP32-S3 ADC absolute-accuracy diagnosis
module: device-control
problem_type: diagnosis
component: firmware-adc
tags:
  - esp32-s3
  - adc1
  - efuse-calibration
  - pt1000
  - measurement-accuracy
status: active
related_specs:
  - docs/specs/adc-calibration-control-plane/SPEC.md
  - docs/specs/real-control-plane-runtime/SPEC.md
---

# ESP32-S3 ADC absolute-accuracy diagnosis

## Context

ESP32-S3 ADC 读数出现启动后变化或与外部温度计不一致时，必须先区分三类问题：

- 电路模型错误，例如分压电源、电阻值或拓扑与网表不一致；
- ADC conversion path 的偏置、增益、参考或供电状态变化；
- 外部模拟节点本身发生变化。

这三类问题不能通过增加平均次数或套用启动补偿曲线混为一谈。绝对精度判断必须有可追溯的已知输入；环境温度、VIN、首次读数和另一条未知 ADC 通道都不是自动成立的校准参考。

## Symptoms

常见表现包括：

- 上电后第一批读数看似合理，随后缓慢移动；
- temperature、curve-calibrated mV 和 raw code 同方向变化；
- ADC1 的两个外部通道一起移动；
- Wi-Fi、Dashboard 请求或外设活动与变化时间接近，但 A/B 结果不一致；
- 固定延迟初始化在部分运行中改善稳定性，却不能重复满足门槛；
- 使用错误的分压电源常量造成稳定且可计算的温度偏差。

## Root cause model

### Circuit projection must match the populated board

PT1000 分压换算属于电路模型，不属于 ADC 校准。Flux Purr 的拓扑是：

`3V3 -> 2.49 kOhm -> RTD_SENSE -> PT1000 -> GND`

`TPS62933` 的 `31.6 kOhm / 10 kOhm` 反馈网络按 `0.8 V` 典型反馈参考给出 `3.328 V` 设计名义值。固件若使用 `3.300 V`，会把同一 RTD 节点电压反算成更高电阻和温度。修正为 `3.328 V` 是对齐网表，不是使用现场读数校准，也不证明组装板电源恰好等于该值。

### Raw code and calibrated mV are not independent references

ESP32-S3 的 eFuse init/reference code 和 curve coefficients 在 ADC 初始化时确定。对相同 12-bit code，固定 curve 必须得到相同 mV。记录同次 conversion 的 code 和 curve mV 可以检查遥测实现，但不能把两者当成两个独立测量源。

如果 ADC transfer function 表示为 `code = F(Vin, ADC state)`，软件只观察 code，无法单独判断：

- 外部 `Vin` 发生变化；
- `Vin` 不变，但 ADC reference、gain、offset 或共享模拟状态发生变化。

第二个未知外部通道只能提供 common-mode 线索，不能替代已知参考。

### ESP32-S3 has no suitable runtime precision reference for this path

ESP32-S3 没有可由软件送到现有 ADC1 measurement path、用于约 `1 V` 增益验证的片上精密参考。内部 GND 路径只能帮助检查 offset；eFuse calibration point 是工厂数据，不是运行时可测电压源。因此，在不改硬件且没有合格外部基准时，软件不能把 common-mode code movement 唯一归因到 ADC 或外部模拟电源。

## Resolution

### Keep the production conversion path explicit

- 使用 `AdcCalBasic<ADC1>` 获得硬件偏置校准后的 code。
- 无条件应用 `0x0fff` mask，避免 SAR 高位状态污染。
- 把同一个 code 交给使用同一 eFuse 参数创建的 `AdcCalCurve<ADC1>` 换算 mV。
- 逐样本换算 mV 后再求均值，避免用第二次 conversion 伪造 code/mV 配对。
- 公开 eFuse version、init/reference code、reference mV 和 retained-batch raw-code statistics，供诊断使用。
- eFuse 数据缺失时报告 `runtime_fallback` 并停止准确性验证；不得用假定 `1100 mV` reference 继续宣称绝对精度。

### Establish a deterministic startup state

ADC 在显示、PD/I2C、持久化恢复、网络任务、PWM safe-off、power capability 读取和 heater-disabled power synchronization 完成后最后初始化。ADC 后不再初始化硬件外设，只允许首次测量、状态投影和发布 ready。

这个顺序减少启动状态的不确定性，但不构成 ADC warm-up 校准。Espressif 没有为 ESP32-S3 ADC1 发布秒级 warm-up 常数或可用于温度补偿的启动曲线。

### Preserve only repeatable fixes

Flux Purr 的重复实验没有证明以下单项可以稳定消除 residual movement：

- ADC measurement-status busy wait；
- 禁用 Wi-Fi；
- 把 ADC 初始化提前到 Wi-Fi 之前；
- Dashboard/LAN request load 隔离；
- 禁用 display refresh、PD polling、fan PWM 或其他单个外设活动；
- 固定 `10` 至 `60` 秒初始化延迟。

这些变体不应保留为产品 feature。固定延迟偶尔改善相对稳定性，但不能证明绝对读数正确，也没有达到一致重复门槛。

## Diagnostic workflow

1. 从网表和器件反馈网络重新计算电路模型，先消除确定性的 projection error。
2. 确认通道、衰减、有效量程、eFuse calibration source 和 fallback 状态。
3. 从同次 conversion 同时记录 12-bit code、curve mV、batch min/max/spread 和 uptime。
4. heater 和主动风扇保持关闭，使用独立完整断电周期做至少三次重复。
5. 每次只改变一个可控变量，例如 Wi-Fi compiled state、RF state、初始化顺序或请求负载。
6. 将结论限制在实验能区分的边界；两个未知输入共同变化只支持 common-mode 判断。
7. 绝对准确度需要已知、独立且带误差预算的电压或温度基准。

## Guardrails and reuse notes

- 不使用环境温度、首次读数、VIN 比例、uptime 曲线或片上温度作为自动校准参考。
- 不用项目 ADC calibration 掩盖错误的分压拓扑常量。
- 增加 averaging 主要降低随机噪声，不能修复相关的秒级或分钟级漂移。
- ESP32-S3 的公开 Wi-Fi 硬冲突针对 ADC2；ADC1 读取成功不能证明 RF 对模拟精度完全无影响。
- 数据手册 ADC 精度条件包含规定的供电、旁路、温度和 Wi-Fi 状态；超出条件时不能直接继承同一误差保证。
- ready gate 最多证明测量链路达到规定的重复状态，不能替代绝对校准。
- 原始 HIL 数据属于任务证据，不应默认进入长期项目文档；长期文档只保存可复用结论和边界。

## References

- [ESP-IDF ADC calibration driver](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/adc/adc_calibration.html)
- [ESP32-S3 datasheet](https://documentation.espressif.com/esp32-s3_datasheet_en.pdf)
- [ESP-IDF ESP32-S3 ADC low-level definitions](https://github.com/espressif/esp-idf/blob/v6.0.2/components/esp_hal_ana_conv/esp32s3/include/hal/adc_ll.h)
- [ESP-IDF ESP32-S3 eFuse ADC calibration source](https://github.com/espressif/esp-idf/blob/v6.0.2/components/efuse/esp32s3/esp_efuse_rtc_calib.c)
- [esp-hal 1.0.0 ADC implementation](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.0.0/esp-hal/src/analog/adc/xtensa.rs)
- [ADC calibration control-plane implementation](../../specs/adc-calibration-control-plane/IMPLEMENTATION.md)
- [Runtime startup implementation](../../specs/real-control-plane-runtime/IMPLEMENTATION.md)
