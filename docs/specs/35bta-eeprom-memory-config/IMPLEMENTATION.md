# EEPROM 记忆配置实现状态（#35bta）

## Coverage

- 固件 `memory` 模块使用 v2 `1024 bytes` active 双槽，位于 `0x0400` / `0x0800`；EEPROM 启动同时读取 active `1024B`、previous `2048B`（`0x1000` / `0x1800`）与 legacy `512B` 双槽并选择最大 sequence，写入只落 active 双槽。
- `flux-purr` runtime 在 CH224Q 请求完成后读取 EEPROM 与 ESP flash fallback，并在 UI 初始绘制前恢复可记忆字段。
- 前面板接受交互后生成新的记忆配置；配置变化会触发约 `2s` debounce，优先写 EEPROM，EEPROM 不可达或写入失败时写入 ESP flash data/NVS 分区末端 8KiB fallback 区。flash 双槽各自占用独立 4KiB erase sector，避免写入期间掉电同时破坏两份 record。
- ADC calibration state 作为 `MemoryConfig` 字段持久化，并在启动后恢复给 RTD/VIN measurement path 和 control-plane response；其中包含共享样本、A/B 槽位与当前激活槽位。
- EEPROM calibration persistence now keeps the ADC-domain pairs and the owner-entered physical references in separate TLVs, so RTD/VIN sample tables can render the original `referenceTempC` / `referenceVinMv` after refresh, reboot, export/import, or devd reconnect.
- EEPROM 读写失败只记录日志并尝试 flash fallback；所有持久化后端都失败时回退默认/当前配置，不阻断 heater/fan 保护。
- `MemoryConfig` 保存 `pps3a` / `pps5a` thermal control bank 与 `thermalProfileMode`。旧单档数据迁入 `pps3a`，缺失 mode 按 `65w` 恢复。
- 新 thermal profile payload 使用 `TCP2` 布局标识；decoder 对无标识 payload 保持历史布局优先，避免仅凭总长度误判 point-local 格式。

## Validation

- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo fmt --manifest-path firmware/Cargo.toml --check`
- 最长 Wi-Fi 凭据、完整 calibration TLV 与六点 thermal profile 的 record round-trip 测试
- 历史五点 profile 与新 point-local profile 总长度碰撞的迁移回归测试
- Xtensa release build按 `SPEC.md` 的质量门槛执行。

## Remaining Work

- HTTP Wi-Fi 配置服务端尚未实现；持久化模型已预留字段。
- EEPROM 地址脚硬件基线是 `0x50`；固件以 `0x50` 为首选并探测 `0x50..0x57`。若 EEPROM 当前不响应，flash fallback 仍提供重启后恢复能力。
