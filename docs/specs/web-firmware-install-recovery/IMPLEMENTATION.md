# Flux Purr Web 固件安装与恢复实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: implemented; devd hardware validation complete, Browser hardware validation pending
- Lifecycle: active
- Delivery: fast-track PR，停在 Step 5C merge-ready

## Coverage / rollout summary

- `flux-purr-bundle` 生成确定性三段包；Rust 与 Browser 校验器共享 schema、布局、migration registry 和 fixtures。
- 固件构建身份与 `commissioningRequired` 已持久化，USB JSONL 提供 `get_install_status` 供写后身份、布局和 setup 状态验收。
- devd bundle API 使用内容寻址导入、五分钟单次 approval token、固定端口/lease/ROM/包/任务绑定、Rust `espflash 4.5.0` 安全探测、配置保全和逐段 ROM MD5。update 在 ROM 探测前从运行时刷新 identity 与 status；独立 ROM 探测在建立连接后始终复位回 runtime。每个 bundle 事务仅在起始进入 ROM 一次，后续擦除、段写入、MD5、配置保全均保持 `no-reset`，全部验证后才复位到 runtime 一次。写后在同一原生串口会话中依次验证 runtime identity 与 install status，并为冷启动保留 45 秒专用验证窗口；验证 I/O 异常直接结算为 `write_complete_unverified`，不得重新打开 USB Serial/JTAG 会话造成额外复位。
- Web 工作台先选择 update 或 install/recovery；两条引擎均执行所选任务的完整门禁。`devd` 仅在 `/health` 可用时成为默认引擎，Browser update 会验证 Flux Purr runtime、停止加热并确认温度不高于 40 C 后进入同一串口的 ROM 预检。Browser 在用户点击同步栈中以 ESP32-S3 USB Serial/JTAG `0x303A:0x1001` 过滤器调用 `requestPort()`，并将点击、选择器、端口结果、运行时、ROM 和预检终态记录到页面事务日志及仅本地下载的诊断报告。Browser 在 ROM bootloader 内读取芯片、Flash package 与 `GET_SECURITY_INFO`，通过后才上传 esptool stub；写入只消费同一连接中已成功的 ROM 预检结果。固件包由单一入口打开组件库对话框，在发布版本和本地 `.fluxpurr-fw` 之间切换。发布版本采用高度为 `min(36rem, 100dvh - 4rem)` 的左右两栏：左侧承载 RC opt-in、当前选择与说明，右侧以共享 `ScrollArea` 独立滚动并占满 tabs 与公共操作区之间的内容高度。列表以 `publishedAt` 倒序展示全部可见版本、不按 channel 分组，非稳定版本以 RC chip 标记；默认选择最新 stable，RC 显式 opt-in。Browser 严格读取同源 `firmware/releases-manifest.json` 和清单中的精确 bundle 路径，并把 bundle SHA-256、version、channel、source SHA 与 build ID 同 manifest 交叉验证。release workflow 在服务器侧同步完整历史 GitHub Releases 并加入当前 bundle；Vite 开发代理将已打包、GitHub 与本地产物以相同的目录合同提供给 Browser，本地产物覆盖相同 build identity 的前序项。另有降级确认和本地诊断下载。
- 控制台将固件维护作为独立根工作区；设备态的顶部顺序为设备选择器、热板、PD、工作区切换，且设备页签只在已选择控制设备时出现。固件态的右侧保留给当前烧录事务日志，不复用设备运行日志。事务日志保留最近 200 条会话记录，并以共享 `ScrollArea` 默认跟随最新条目；操作者上滚历史后可通过图标入口回到最新日志。
- release workflow 与 `bun run build:firmware:web` 都使用锁定的 `espflash 4.5.0` 镜像格式库，把 ELF 直接生成三段 `.fluxpurr-fw`；前者输出 firmware tarball 与 release bundle，后者固定输出到 `firmware/target/flux-purr-web-artifacts/`。Vite 监听该默认目录并仅以内存提供相同字节，不创建静态副本。产品 manifest 记录 channel、media type、size 和 SHA-256。release Web bundle 包含完整同源 firmware release catalog，并在 GitHub Release 创建后部署到 EdgeOne。
- devd 已在主人授权的单一精确串口上完成 `install_recovery` 与同布局 `update` HIL：全擦恢复、配置保全、三段镜像写入、ROM MD5、一次最终复位和 runtime identity/install-status 均返回 `verified`。更新后固件版本、完整 source SHA、build ID 与 layout 一致，EEPROM 有效记录 sequence 保持为 `3207`；延时状态检查确认 uptime 持续增长、加热输出为零，事件中没有 panic、`StoreProhibited` 或 `IllegalInstruction`。Browser HIL 仍独立验收，且不得以 devd 结果替代。

## Validation

- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml --lib`
- `cargo test --manifest-path firmware/Cargo.toml --lib control_plane`
- `bun run typecheck`, `bun run check`, installer unit tests, `ui_demo` visual review and E2E coverage
- `.github/scripts/test-release-labels.sh`

## Remaining Gate

- Browser 真机恢复、更新和主动中断恢复仍需独立取得精确端口和相应写入授权后执行。

## References

- `./SPEC.md`
- `./HISTORY.md`
