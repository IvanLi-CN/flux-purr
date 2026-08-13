# EEPROM 记忆配置实现状态（#35bta）

## Coverage

- 固件 `memory` 模块当前写入 `MemoryRecord` v5：`2048 bytes` active 双槽位于 `0x1000` / `0x1800`，使用 `u16` TLV 长度；解码兼容 v1-v5。EEPROM 启动同时读取 active `2048B`、previous `1024B`（`0x0400` / `0x0800`）与 legacy `512B` 双槽并选择最大 sequence。旧 EEPROM record 只在 RAM 中迁移，后续成功提交才物化为 v5 并写入 active 槽。
- `flux-purr` runtime 在 CH224Q 请求完成后读取 EEPROM 与 ESP flash fallback，并在 UI 初始绘制前恢复可记忆字段。
- 前面板接受交互后生成新的记忆配置；配置变化会触发约 `2s` debounce，优先写 EEPROM，EEPROM 不可达或写入失败时写入专用 `flux_cfg` 8KiB data partition。flash 双槽各自占用独立 4KiB erase sector，避免写入期间掉电同时破坏两份 record，也不绕过 NVS allocator 写入 NVS 管理范围。
- runtime 先读取当前 `flux_cfg`；没有有效 record 时只读探测旧 factory-app 边界后的 `0x110000` / `0x111000` raw fallback 双槽。发现 CRC 合法 record 后立即复制到当前分区，后续写入只使用 `flux_cfg`。
- `devd` real flash 在取得 serial 排他锁后，先读取设备的真实 partition table。若目标 `flux_cfg` 地址变化，daemon 读取完整当前 `flux_cfg` 或未分区 legacy raw 双槽，在目标地址预写并逐字读回验证；目标地址被当前 partition 占用、容量不足或任意 read/write/verify 失败时，daemon 在 app 写入前拒绝操作。临时副本仅存在于受限临时目录，不会写入日志、trace 或本地设备记录。
- ADC calibration state 作为 `MemoryConfig` 字段持久化，并在启动后恢复给 RTD/VIN measurement path 和 control-plane response；其中包含共享样本、A/B 槽位与当前激活槽位。
- EEPROM calibration persistence now keeps the ADC-domain pairs and the owner-entered physical references in separate TLVs, so RTD/VIN sample tables can render the original `referenceTempC` / `referenceVinMv` after refresh, reboot, export/import, or devd reconnect.
- EEPROM 读写失败只记录日志并尝试 flash fallback；所有持久化后端都失败时回退默认/当前配置，不阻断 heater/fan 保护。
- `MemoryConfig` 保存 `pps3a` / `pps5a` thermal control bank 与 `thermalProfileMode`。旧单档数据迁入 `pps3a`，缺失 mode 按 `65w` 恢复。
- 新 thermal profile payload 使用 `TCP2` 布局标识；decoder 对无标识 payload 保持历史布局优先，避免仅凭总长度误判 point-local 格式。

## Validation

- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo fmt --manifest-path firmware/Cargo.toml --check`
- 最长 Wi-Fi 凭据、完整 calibration TLV 与双 bank 各 10 点 thermal profile 的 v5 record round-trip 测试
- v1-v4 record header/TLV 解码、旧 calibration/profile 字段迁移与 v5 sanitize 归一化测试
- 历史五点 profile 与新 point-local profile 总长度碰撞的迁移回归测试
- Xtensa release build按 `SPEC.md` 的质量门槛执行。

## Remaining Work

- HTTP Wi-Fi 配置服务端尚未实现；持久化模型已预留字段。
- EEPROM 地址脚硬件基线固定为 `0x50`；固件只访问该地址，不扫描共享 I2C 总线。启动读取复用静态 scratch 并采用有界分块，避免把完整 record buffer 放入启动栈。若 EEPROM 当前不响应，flash fallback 仍提供重启后恢复能力。
- EEPROM 含有非空但不可解析、CRC/结构无效或未来格式数据时，固件保持 heater/PPS/calibration 锁定，并由前面板显示统一的不兼容故障场景。仓库 devd CLI 通过 USB lease 提供完整 8KiB 原始导出、原样导入与全片 `0xFF` 擦除回读；该高级兜底不进入 Web/LAN，也不解析或过滤内容。
