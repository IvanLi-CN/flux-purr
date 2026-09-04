# Flux Purr Web 固件安装与恢复演进历史

> 这里记录影响长期设计理解的关键原因；规范正文以 `./SPEC.md` 为准。

## Decision Trace

- 现有 Web Serial 运行时 JSONL 与 devd-only flash 不能覆盖空片或外来固件 DIY 场景。
- 产品选择一个 bundle 合同、两个执行引擎，而不是让 Browser 和 devd 分别解释 firmware artifacts。
- install/recovery 不依赖 Flux runtime 身份，并明确禁止 PCB/heater 物理状态推断或限制；update 仍要求软件可观测的停热与温度门禁。
- EEPROM 位于 MCU internal Flash 之外；full erase 只作用于 internal Flash。原先的 `flux_cfg` layout migration 已由 [`ADR 0008`](../../adr/0008-eeprom-only-configuration-persistence.md) 取代，当前合同不迁移 MCU 配置。
- 原先未签名 bundle 的信任模型已由 [`ADR 0007`](../../adr/0007-firmware-update-and-developer-flash-boundaries.md) 取代，当前合同要求产品发布签名。
- partition-table source binary is padded with `0xff` to its exact 4 KiB flash segment before hashing and packaging, so layout identity always describes written bytes.
- Browser 直接读取 GitHub API 会受 CORS、重定向和共享限流影响，因此 GitHub 只作为 release build 与 Vite 开发代理的服务器端来源；Browser 只解释统一的同源静态目录。开发时本地产物覆盖相同 build identity 的已发布 artifact，既保留完整发布时间线，也避免本地调试误选旧 bytes。
- ESP32-S3 软件复位不会清空 NOLOAD heap 区域；运行时在注册该区域前必须显式清零，避免 Wi-Fi C 驱动把残留字节解释为有效 timer 指针。持久化记录必须原地解码到单一配置对象，避免嵌套的大值返回耗尽启动栈；显示与 I2C 启动 I/O 必须有有限超时，使 USB 恢复控制面始终可达。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
