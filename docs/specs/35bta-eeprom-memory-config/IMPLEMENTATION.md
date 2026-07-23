# EEPROM 记忆配置实现状态（#35bta）

## Coverage

- 固件 `memory` 模块使用 v2 `1024 bytes` active 双槽，位于 `0x0400` / `0x0800`；EEPROM 启动按 `1024B v2 -> 512B legacy` 顺序读取，写入只落 v2 槽。
- `flux-purr` runtime 在 CH224Q 请求完成后读取 EEPROM 与 ESP flash fallback，并在 UI 初始绘制前恢复可记忆字段。
- 前面板接受交互后生成新的记忆配置；配置变化会触发约 `2s` debounce，优先写 EEPROM，EEPROM 不可达或写入失败时写入 ESP flash data/NVS 分区末端 fallback 区。
- ADC calibration state 作为 `MemoryConfig` 字段持久化，并在启动后恢复给 RTD/VIN measurement path 和 control-plane response；其中包含共享样本、A/B 槽位与当前激活槽位。
- EEPROM calibration persistence now keeps the ADC-domain pairs and the owner-entered physical references in separate TLVs, so RTD/VIN sample tables can render the original `referenceTempC` / `referenceVinMv` after refresh, reboot, export/import, or devd reconnect.
- EEPROM 读写失败只记录日志并尝试 flash fallback；所有持久化后端都失败时回退默认/当前配置，不阻断 heater/fan 保护。
- `MemoryConfig` 保存 `pps3a` / `pps5a` thermal control bank 与 `thermalProfileMode`。旧单档数据迁入 `pps3a`，缺失 mode 按 `65w` 恢复。

## Validation

- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo fmt --manifest-path firmware/Cargo.toml --check`
- 最长 Wi-Fi 凭据、完整 calibration TLV 与六点 thermal profile 的 record round-trip 测试
- Xtensa release build按 `SPEC.md` 的质量门槛执行。

## Remaining Work

- HTTP Wi-Fi 配置服务端尚未实现；持久化模型已预留字段。
- EEPROM 地址脚硬件基线是 `0x50`；固件以 `0x50` 为首选并探测 `0x50..0x57`。若 EEPROM 当前不响应，flash fallback 仍提供重启后恢复能力。
