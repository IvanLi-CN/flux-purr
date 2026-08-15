# Flux Purr Web 固件安装与恢复演进历史

> 这里记录影响长期设计理解的关键原因；规范正文以 `./SPEC.md` 为准。

## Decision Trace

- 现有 Web Serial 运行时 JSONL 与 devd-only flash 不能覆盖空片或外来固件 DIY 场景。
- 产品选择一个 bundle 合同、两个执行引擎，而不是让 Browser 和 devd 分别解释 firmware artifacts。
- install/recovery 不依赖 Flux runtime 身份，并明确禁止 PCB/heater 物理状态推断或限制；update 仍要求软件可观测的停热与温度门禁。
- EEPROM 位于 MCU internal Flash 之外；full erase 只作用于 internal Flash。`flux_cfg` 通过精确 layout migration 在 update 中保全。
- v1 不引入签名，把来源可信度留给 HTTPS/GitHub Release 和公开 SHA-256。
- partition-table source binary is padded with `0xff` to its exact 4 KiB flash segment before hashing and packaging, so layout identity always describes written bytes.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
