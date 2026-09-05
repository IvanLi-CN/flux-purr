# Flux Purr Web 固件安装与恢复实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: Web bundle validation, release-scoped SHA-256 catalog checks, and EEPROM-Only Persistence are implemented.
- Lifecycle: active
- Delivery: fast-track PR，停在 Step 5C merge-ready

## Coverage / rollout summary

- `flux-purr-bundle` 生成确定性四文件 bundle v2；Rust 与 Browser 校验器共享 schema、布局和 fixtures，发布清单提供无签名 SHA-256 完整性信任。
- 固件构建身份与 `commissioningRequired` 已持久化，USB JSONL 提供 `get_install_status` 供写后身份、布局和 setup 状态验收。
- `get_install_status` 的 `setupReason` 在 commissioning 已完成时按合同返回 `null`，devd 以可选字段解码；`GET /api/v1/devices/{deviceId}/install-status` 以 active exact-port lease 代理该只读 USB JSONL 请求。固件维护目标以已授权 native serial candidate 的 devd 能力为准，不因运行时 identity 不声明 `flash` 而在写后丢失目标。
- devd bundle API uses content-addressed import and per-segment ROM MD5; CLI-to-devd control uses the versioned local CBOR socket. MCU internal configuration is never staged or restored, and release trust comes only from the published SHA-256 integrity catalog.
- devd 在现有设备 SSE 上以 `firmware_operation` 发布 operation-scoped 的真实阶段事件，preflight 与 execution 使用不同 operation ID 和步骤集合；Web 按 operation ID 与 sequence 去重，并将预检进度和执行进度分别结算。执行只有在最终 outcome 为 `verified` 时达到 100%，写完但运行时未验证保持低于 100%。
- Web 工作台先选择 update 或 install/recovery；两条引擎均执行所选任务的完整门禁。`devd` 仅在 `/health` 可用时成为默认引擎，Browser update 会验证 Flux Purr runtime、停止加热并确认温度不高于 40 C 后进入同一串口的 ROM 预检。Browser 预先只读读取 `getPorts()`；唯一已授权且精确匹配 ESP32-S3 USB Serial/JTAG `0x303A:0x1001` 的端口默认复用，不会重复打开选择器。Browser USB 连接区提供组件库按钮“选择 / 更换浏览器 USB 端口”：用户点击后立即在同步栈打开同一过滤器的 Chrome 选择器，选择结果固定为下一次预检的唯一目标并使旧预检凭据失效；取消或失败不选择其他已授权对象。授权绑定同一 Chrome profile 的 `scheme + host + port`；清除站点数据、变更 profile 或在开发中改用其他 loopback host/端口会回到用户选择流程。未授权、信息不匹配或存在歧义时，预检同样在用户点击同步栈中以该过滤器调用 `requestPort()`。页面将点击、授权端口复用或选择器、端口结果、运行时、ROM 和预检终态记录到页面事务日志及仅本地下载的诊断报告。Browser 在 ROM bootloader 内读取芯片、Flash package 与 `GET_SECURITY_INFO`，通过后才上传 esptool stub；写入只消费同一连接中已成功的 ROM 预检结果。固件包由单一入口打开组件库对话框，在发布版本和本地 `.fluxpurr-fw` 之间切换。发布版本采用高度为 `min(36rem, 100dvh - 4rem)` 的左右两栏：左侧承载 RC opt-in、当前选择与说明，右侧以共享 `ScrollArea` 独立滚动并占满 tabs 与公共操作区之间的内容高度。列表以 `publishedAt` 倒序展示全部可见版本、不按 channel 分组，非稳定版本以 RC chip 标记；默认选择最新 stable，RC 显式 opt-in。Browser 严格读取同源 `firmware/releases-manifest.json` 和清单中的精确 bundle 路径，并把 bundle SHA-256、version、channel、source SHA 与 build ID 同 manifest 交叉验证。release workflow 在服务器侧同步完整历史 GitHub Releases 并加入当前 bundle；Vite 开发代理将已打包、GitHub 与本地产物以相同的目录合同提供给 Browser，本地产物覆盖相同 build identity 的前序项。另有降级确认和本地诊断下载。
- Browser 写入阶段由 esptool 的真实擦除、分段字节、ROM MD5 和复位回调驱动。ESP32-S3 原生 USB Serial/JTAG 的 stub 交接使用 `ESP_MEM_END` 的 2 秒超时，写后按 `hard_reset` 再 `D0|R0|W50|D1|R0|W50|D0|R1|W50|D0|R0|W250` 返回应用并释放 ROM transport。运行时验证在 45 秒总窗口内处理 CDC 重枚举、暂时占用、启动文本和 JSONL identity/install-status 重试：原 `SerialPort` 对象失效时，只从同一 origin 已授权集合中重新解析唯一精确匹配的 `0x303A:0x1001` 对象；零个或多个匹配均 fail closed，绝不调用 `requestPort()` 或选择其他候选。所有断开、打开、读写、取消和关闭都有界，超时清理流锁并结算 `write_complete_unverified`。
- `demo=true`、`uiDemo=firmware-workspace` 与发布 public demo 统一使用与正式运行时分离的 mock 执行模式：目录、固件、本地导入、USB、ROM/security 与事务结果均由内存样本提供。所有这些路径硬性关闭 devd、Web Serial、业务 fetch 与跨源资源加载，且以“演示本地包”替代系统文件选择器；预检与写入只推进可审计的模拟状态机。`uiDemo` 仅为开发期视觉证据路径，发布 public demo 会剥离该参数。
- 控制台将固件维护作为独立根工作区；设备态的顶部顺序为设备选择器、热板、PD、工作区切换，且设备页签只在已选择控制设备时出现。固件态的右侧保留给当前烧录事务日志，不复用设备运行日志。事务日志保留最近 200 条会话记录，并以共享 `ScrollArea` 默认跟随最新条目；操作者上滚历史后可通过图标入口回到最新日志。
- release workflow 与 `bun run build:firmware:web` 都使用锁定的 `espflash 4.5.0` 镜像格式库，把 ELF 直接生成三段 `.fluxpurr-fw`；前者输出 firmware tarball 与 release bundle，后者固定输出到 `firmware/target/flux-purr-web-artifacts/`。Vite 监听该默认目录并仅以内存提供相同字节，不创建静态副本。产品 manifest 记录 channel、media type、size 和 SHA-256。release Web bundle 包含完整同源 firmware release catalog，并在 GitHub Release 创建后部署到 EdgeOne。
- Previous devd HIL exercised superseded MCU configuration preservation and is not acceptance evidence for the current CLI or EEPROM-only boundaries. No real-device or Browser HIL is executed in this change.

## Validation

- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml --lib`
- `cargo test --manifest-path firmware/Cargo.toml --lib control_plane`
- `bun run typecheck`, `bun run check`, installer unit tests, `ui_demo` visual review and E2E coverage
- `.github/scripts/test-release-chain.sh`

## Remaining Gate

- Browser 真机恢复、更新和主动中断恢复仍需独立取得精确端口和相应写入授权后执行。
- The remaining real-device update, flash, recovery, and Browser HIL checks require a separately authorized exact serial port; they are intentionally not run here.

## References

- `./SPEC.md`
- `./HISTORY.md`
