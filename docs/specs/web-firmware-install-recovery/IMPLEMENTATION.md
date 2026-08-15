# Flux Purr Web 固件安装与恢复实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: implemented; hardware validation pending owner authorization
- Lifecycle: active
- Delivery: fast-track PR，停在 Step 5C merge-ready

## Coverage / rollout summary

- `flux-purr-bundle` 生成确定性三段包；Rust 与 Browser 校验器共享 schema、布局、migration registry 和 fixtures。
- 固件构建身份与 `commissioningRequired` 已持久化，USB JSONL 提供 `get_install_status` 供写后身份、布局和 setup 状态验收。
- devd bundle API 使用内容寻址导入、五分钟单次 approval token、固定端口/lease/ROM/包/任务绑定、Rust `espflash 4.5.0` 安全探测、配置保全和逐段 ROM MD5。
- Web 工作台以任务优先展示 update 与 install/recovery，自动优先 devd，支持 Browser Web Serial、stable/RC GitHub Release、本地包、降级确认和本地诊断下载。
- release workflow 用 pinned `espflash 4.5.0 save-image --merge --skip-padding` 同时生成 firmware tarball 与 `.fluxpurr-fw`；product manifest 记录 channel、media type、size 和 SHA-256。
- 真实串口和全擦 HIL 尚未获精确端口授权，不属于当前非硬件验证。

## Validation

- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml --lib`
- `cargo test --manifest-path firmware/Cargo.toml --lib control_plane`
- `bun run typecheck`, `bun run check`, installer unit tests and FirmwareWorkbench Storybook plays
- `.github/scripts/test-release-labels.sh`

## Remaining Gate

- Browser 与 devd 真机恢复、更新和主动中断恢复只在主人提供完整串口路径并明确授权全擦该目标后执行。

## References

- `./SPEC.md`
- `./HISTORY.md`
