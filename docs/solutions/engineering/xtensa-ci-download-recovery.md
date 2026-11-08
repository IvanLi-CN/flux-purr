---
title: Xtensa CI 下载失败与半安装恢复
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

# Xtensa CI 下载失败与半安装恢复

## 故障证据

PR #140 的 CI run `42464863466` 在两次运行中均遇到工具链下载 HTTP 504。第二次已成功取得 espup，但 Xtensa Rust 和 GCC 下载失败；espup 将失败前创建的 GCC 目录报告为可复用安装。固定版本的上游 release 及资产仍存在，本机稍后访问返回 200，不能把这一结果当作 runner 网络已恢复的证明。

espup v0.16.0 的内部安装重试间隔为 50ms，不能覆盖持续数秒的网关故障；仅重新运行整个 CI 还会重复已经成功的检查。相关上游实现为 [安装重试](https://github.com/esp-rs/espup/blob/v0.16.0/src/toolchain/mod.rs)、[GCC 目录判定](https://github.com/esp-rs/espup/blob/v0.16.0/src/toolchain/gcc.rs) 和 [卸载入口](https://github.com/esp-rs/espup/blob/v0.16.0/src/main.rs)。

## 恢复边界

共享 setup action 保持 espup 和 Xtensa Rust 版本固定。安装 wrapper 只对明确的 HTTP 408、429、500、502、503、504，以及连接和 DNS 传输错误作有限恢复：最多三次安装，依次退避 15 秒、45 秒。重试前调用 `espup uninstall --name esp` 清除临时 runner 中对应工具链的半安装目录，避免把存在的目录当作完整安装。

wrapper 只允许在 GitHub Actions job 执行；不在开发者主机执行真实安装或卸载。非网络错误立即返回原错误；清理失败停止重试，日志管道失败不能返回成功。三次失败仍阻断后续步骤，既有 Xtensa Clippy、完整产品构建和安全门禁继续执行。PR、main 和产品发布共用这个 action，不改变 workflow 的触发事件、检查名或 required-check 策略。

## 验证

`python3 -m unittest discover -s .github/scripts -p test_setup_xtensa.py` 使用临时目录及模拟 espup、sleep、tee，覆盖首次成功、504 后清理并恢复、持续失败上限、非网络错误、清理失败、日志写入失败和非 CI 主机拒绝。它不安装任何全局工具，也不代表远端下载已恢复；真实工作流必须继续通过固定版本工具链的安装、Clippy 和构建，才能解除 CI 门禁。
