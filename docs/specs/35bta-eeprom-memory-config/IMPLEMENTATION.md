# EEPROM 记忆配置实现状态（#35bta）

## Coverage

- 固件新增 `memory` 模块，包含 `MemoryConfig`、TLV record 编解码、CRC 校验、双槽选择和 M24C64 adapter。
- `flux-purr` runtime 在 CH224Q 请求完成后读取 EEPROM，并在 UI 初始绘制前恢复可记忆字段。
- 前面板接受交互后生成新的记忆配置；配置变化会触发约 `2s` debounce，再写入下一 EEPROM 槽。
- EEPROM 读写失败只记录日志并回退默认/当前配置，不阻断 heater/fan 保护。

## Validation

- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo fmt --manifest-path firmware/Cargo.toml --check`
- Xtensa release build按 `SPEC.md` 的质量门槛执行。

## Remaining Work

- HTTP Wi-Fi 配置服务端尚未实现；持久化模型已预留字段。
- EEPROM 地址脚若实板不是 `0x50`，需要调整 `M24C64_I2C_ADDRESS`。
