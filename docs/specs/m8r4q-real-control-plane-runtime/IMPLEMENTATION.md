# Flux Purr 真实控制平面运行时实现状态（#m8r4q）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: 部分完成
- Lifecycle: active
- Catalog note: Web + firmware + native devd real transport contract

## Coverage / rollout summary

- 共享领域契约由 firmware、devd 与 Web 各自的 typed adapter 实现。
- firmware v1 先交付 host-testable status adapter、USB JSONL parser/encoder、WiFi redaction 与 feature flags。
- `tools/flux-purr-devd` 提供 localhost daemon、mock/serial scan、lease、bounded events、artifact verify、dry-run 与 flash command boundary。
- Web demo 保持 `#hhwq8` 轻量 bench console，新增 transport client、capability gate 与 Storybook scenarios。

## Remaining Gaps

- 真机 USB CDC、WiFi HTTP handoff 与 espflash write 需要主人后续切换工作区并触发实机 smoke 后记录。
- PR 号在 PR 创建后回填。

## References

- `./SPEC.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
