# Flux Purr 单一产品版本源

> 本文定义产品版本事实与 build identity。发布决策、trusted baseline、release intent、tag reservation 和 recovery 由 [Manual version release delivery](../manual-version-release-delivery/SPEC.md) 定义。

## Goals

- 根目录 `VERSION` 是 firmware、devd、CLI、Web、bundle、manifest 和 Release 的唯一产品版本事实。
- 开发构建只读 `VERSION`，显示 `nextPatch(VERSION)-dev.<short-sha>`，不修改源码树。
- manual release 在 runner 的 detached workspace 写入 workflow 独立解析的 canonical version，再由同一 workspace 构建所有产品资产。
- 所有 consumer 使用 `scripts/product-version.py` 的同一解析规则；Git tags、package metadata、PR labels、merge order 和 workflow channel 不提供版本回退或覆盖。

## Version file contract

根 `VERSION` 必须存在、UTF-8 编码、只有一行、恰好一个 trailing LF，并符合 [version-file contract](./contracts/version-file.md)。允许稳定 SemVer `X.Y.Z` 与 RC `X.Y.Z-rc.N`，不允许注释、额外空行、开发后缀或 package metadata fallback。

## Build modes

### Development

`product-version.py --mode development` 读取 VERSION，解析 `nextPatch(VERSION)`，拼接当前 source SHA 的短值，得到 `X.Y.Z-dev.<short-sha>`；channel 为 `local`。该过程不得写 VERSION。

### Release

`product-version.py --mode release` 读取 runner workspace 的 VERSION 原文；stable channel 为 `stable`，RC channel 为 `rc`。manual release workflow 在构建前将 Version Policy 解析出的 canonical version 写入该 workspace，不能把 caller 输入或 Agent 建议当成版本事实。

## Consumer contract

下列 surface 必须表达同一 `product_version` 与 source SHA：

- firmware build metadata 与 USB identity；
- `flux-purr-devd` health、CLI `--version` 与 host tools；
- Web build metadata 与 firmware catalog；
- firmware bundle、release manifest、Git tag、GitHub Release asset names。

同一 Release 中每种 artifact 只构建一次，manifest 的 `sourceSha` 必须是 manual dispatch 固定的 `main` exact SHA。部署使用已验证 artifact，不重新解析版本或重新构建。

## Release boundary

版本选择不发生在 PR 上。Agent 只在 owner 确认前提出 intent 与预期版本建议；owner 选择 `patch`、`minor`、`major` 或 `rc` 后，workflow 按 trusted stable baseline 独立解析 canonical version。PR 不创建 VERSION-only preparation commit，CI PR/CI Main 不识别 prepared commit。

非纯文档变更默认要求真实产品 Release。只有 `docs/**`、`README*`、`CHANGELOG*`、Markdown、纯文本和文档图片资源组成的变更可以被 workflow 明确 skip；混合变更按产品变更处理。

## Failure and recovery

- 缺失或非法 VERSION 必须 fail closed，不得回退到 `0.1.0`、package manifest 或最近 tag。
- stable/RC canonical version、tag、artifact、Release 和 deployment 由同一个 release identity 绑定。
- publish 在构建前 reservation tag；collision 失败并要求 owner 选择新的 intent 或确认是否为同 identity recovery。
- recovery 只接受已存在且精确指向当前 source SHA 的 release tag，不接受旧的 `commit_sha` 输入，不重算 successor、不递增 RC 序号、不移动 tag。

## Non-goals

- 不迁移或重写历史 tag、GitHub Release 或已发布资产。
- 不改变 firmware、Web、devd 或 control-plane 功能。
- `SemVer change governance` 只作为可选 compatibility assessment，不决定 release target、release eligibility 或 RC serial。

## Related contracts

- [Manual version release delivery](../manual-version-release-delivery/SPEC.md)
- [Version file](./contracts/version-file.md)
- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [ADR 0025](../../adr/0025-manual-version-release-delivery.md)
- [Release Product workflow](../../../.github/workflows/release.yml)
