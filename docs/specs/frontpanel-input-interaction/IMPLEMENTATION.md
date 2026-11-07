# Frontpanel input interaction implementation

## Current coverage

- Five-way input decoding, gesture thresholds, menu routing, Key Test diagnostics, and dashboard navigation are implemented in the frontpanel runtime.
- Host-side framebuffer fixtures and interaction evidence cover Dashboard, Menu, Preset Temp, Active Cooling, and Device Info states.
- Persistence fault and acknowledged states remain product renderer states; physical front-panel checks use `flux-purr ram-run preview frontpanel` and never write EEPROM.
- Heater and fan runtime semantics are consumed from the dedicated heater runtime topic.

- 原始按下候选在释放稳定前保持有效，单次采样可见的快速点击不会因尚未达到稳定按下阈值而丢失；释放去抖为 `5ms`。

## Validation

- Product firmware tests cover gesture and route transitions.
- Storybook and visual evidence cover the canonical input and navigation states.

- 回归测试覆盖五个原始键、`5ms/20ms` 去抖配置下的单采样点击、释放抖动合并及既有双击、长按与 repeat 互斥。
- 主人在精确授权端口完成修复固件烧录后的实机操作，确认所有按键快速短按恢复正常；仅降低去抖阈值的先前产物未解决问题。开发者烧录采用明确授权的无 EEPROM 备份旁路。

## Remaining gaps

- Physical-device input and GPIO acceptance requires separately authorized hardware validation.
