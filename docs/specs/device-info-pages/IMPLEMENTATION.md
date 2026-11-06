# 设备信息三页实现说明

## 当前覆盖

固件 `DeviceInfoPage` 已提供 Identity、Calibration、Runtime 三页，使用产品 renderer 生成 160×50 RGB565 framebuffer；host preview 通过正式 renderer 输出三页深色与浅色场景。Web 前面板预览提供同名页面和手动翻页模型。

## 验证

实现验收以 `firmware/src/frontpanel/render.rs` 的 host 测试、`device_info_candidates` 的正式 framebuffer 输出、Web 类型检查和不可变 PNG 快照为证据。边界检查必须覆盖长版本、未知状态、长复位原因和 `HEATER 100%`。

## 剩余工作

若固件或 Web 的坐标、字级、状态文案发生变化，必须先更新 `SPEC.md` 的规范表，再同步两侧 renderer 与证据。真机显示和按键验收属于独立授权的硬件门禁。
