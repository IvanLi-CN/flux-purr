---
title: Xtensa CI 官方资产下载与完整安装
module: ci-toolchain
problem_type: workflow
component: github-actions
tags:
  - xtensa
  - espup
  - github-actions
status: active
related_specs: []
---

# Xtensa CI 官方资产下载与完整安装

## 故障证据

PR #140 的 CI runs `42464863466`、`42467853689` 和 `42469057328` 在 espup ZIP 或内部 Rust、GCC、LLVM 下载阶段持续遇到 HTTP 504。有限重试和半安装清理未恢复普通 release 下载地址。最后一个 run 的第二次 attempt 已证明：同一 espup 资产通过官方 API 成功取得并通过摘要校验，但随后内部普通 release URL 仍失败。这是下载路径故障，不能当作固件编译错误，也不能通过跳过检查处理。

espup v0.16.0 的内部安装重试间隔为 50ms，GCC 仅根据目录存在判断能否复用。相关上游实现为 [下载与重试](https://github.com/esp-rs/espup/blob/v0.16.0/src/toolchain/mod.rs)、[Rust 安装](https://github.com/esp-rs/espup/blob/v0.16.0/src/toolchain/rust.rs)、[GCC 版本与布局](https://github.com/esp-rs/espup/blob/v0.16.0/src/toolchain/gcc.rs) 和 [LLVM 版本与布局](https://github.com/esp-rs/espup/blob/v0.16.0/src/toolchain/llvm.rs)。

## 安装边界

共享 action 直接从官方 GitHub release asset API 取得同一组固定组件：Rust `1.95.0.0`、rust-src、GCC `15.2.0_20250920`、LLVM `esp-20.1.1_20250829` 的 libs 包。`.github/actions/setup-xtensa/toolchain.py` 固定官方仓库、资产 ID、摘要和预期归档根目录；更换工具链必须显式更新该清单。资产来源是 [Rust release](https://github.com/esp-rs/rust-build/releases/tag/v1.95.0.0)、[GCC release](https://github.com/espressif/crosstool-NG/releases/tag/esp-15.2.0_20250920) 和 [LLVM release](https://github.com/espressif/llvm-project/releases/tag/esp-20.1.1_20250829)。

四份资产并行下载，各自使用独立临时文件，API 传输超时或明确 408、429、500、502、503、504 最多尝试三次，退避 5 秒、15 秒。权限或配置错误立即失败。每份下载以固定 SHA-256 校验后原子发布；全部通过后才解压，归档根目录和 data filter 限制路径及链接逃逸。Rust 与 rust-src 使用包内官方 `install.sh`，随后通过 rustup 将 `esp` 链接至该 job 的安装目录；版本、GCC 和 libclang 检查通过后才写环境导出文件。token 只通过环境传给 `gh`，不写入 URL、参数或日志。

安装器只接受 GitHub-hosted Linux x86_64 的固定版本与 ESP32-S3 目标，安装目录位于 `RUNNER_TEMP`。已有目录直接失败，不卸载、不覆盖、不复用半安装；失败不会发布成功环境。PR、main 和产品发布共用该 action；检查名、触发策略、Xtensa Clippy、完整产品构建及安全门禁保持不变。

## 验证

`python3 -m unittest discover -s .github/scripts -p 'test_setup_xtensa*.py'` 使用临时目录和模拟 API、安装命令，覆盖 API 504 与超时恢复、有限重试、权限失败、摘要不符、归档逃逸、全部校验先于安装、安装及版本失败、半安装拒绝和非 hosted CI 拒绝。测试不安装全局工具；真实工作流仍须安装并执行固定工具链的 Clippy、构建和既有安全检查，才能解除 CI 门禁。
