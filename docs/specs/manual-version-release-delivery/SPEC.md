# Flux Purr Manual version release delivery

> 本主题定义 Flux Purr 的手动产品发布契约。实现状态见 `./IMPLEMENTATION.md`，演进背景见 `./HISTORY.md`，不可逆取舍见 [ADR 0025](../../adr/0025-manual-version-release-delivery.md)。

## 目标与范围

- `Release Product` 只通过 `workflow_dispatch` 进入，调用者只能选择 `patch`、`minor`、`major` 或 `rc` 四种 `release_target`，不能提交最终 canonical version。
- Agent 在 dispatch 前读取当前 `main` exact SHA、trusted stable baseline 和变更范围，向 owner 建议 release intent 与预期版本；owner 确认 intent 后才能 dispatch。Agent 的建议不写入仓库、不持久化为 release state，workflow 始终独立重算版本。
- 非纯文档变更默认需要实际产品 Release。纯文档 allowlist 只包含 `docs/**`、`README*`、`CHANGELOG*`、Markdown、纯文本和文档图片资源；混合提交按产品变更处理。
- 根 `VERSION` 仍是 firmware、devd、CLI、Web、bundle、manifest 和 Release 的产品版本事实。manual release 在 runner 的 detached workspace 写入解析结果，不向 PR 分支或 `main` 提交 VERSION。
- PR、review、required checks、signed commits 和 PR-only `main` protection 保持不变；manual release 不能直接写 `main`。

## Version Policy

### Inputs

| 输入 | 来源 | 规则 |
| --- | --- | --- |
| `release_target` | owner-confirmed manual dispatch input | 只接受 `patch`、`minor`、`major`、`rc` |
| source SHA | `workflow_dispatch` event 的 `github.sha` | workflow pin 到该 SHA，并在构建前确认 `origin/main` 尚未前进；漂移时 fail closed |
| trusted baseline | source SHA 可达的最高稳定 `vX.Y.Z` tag | 只接受稳定 final tag；没有历史 tag 时才使用 root `VERSION` 作为 bootstrap fallback |
| tag inventory | fetched immutable tags 与 annotated intent metadata | 用于 collision、RC 序号和 recovery identity 验证 |

caller 不得提供 `version`、tag、commit SHA 或其它字段覆盖 policy 结果。`SemVer change governance` 只在另有兼容性评估需求时作为可选输入，永远不决定 release eligibility、target 或 RC 序号。

### Resolution

- `patch`：trusted baseline 的 patch 加一。
- `minor`：trusted baseline 的 minor 加一、patch 归零。
- `major`：trusted baseline 的 major 加一、minor 和 patch 归零。
- `rc`：先对 trusted final baseline 做 patch 加一，再取该稳定基线下一个未占用的 `rc.N`；例如 `0.30.7` 解析为 `0.30.8-rc.1`。
- stable target 不允许回退或覆盖已存在 tag；RC 序号只在建立新 identity 时解析。相同 identity 的 recovery 不重新计算 successor，也不递增 `N`。

### Release identity

有效 identity 至少绑定：`source_sha`、trusted baseline、owner intent、canonical version、reservation id、tag、artifact digests、GitHub Release、部署结果和 recovery target。reservation id 的当前实现为 `<tag>@<source_sha>`。

`publish` 必须先检查 tag collision，然后创建并验证指向 exact source SHA 的 annotated tag；tag annotation 必须持久化 owner intent、trusted baseline 和 reservation id，再构建资产。`recover` 不接受历史 `commit_sha` 入口，只能复用已经指向当前 exact source SHA 且 intent metadata 与 caller 一致的唯一 release tag；stable intent 不能恢复 RC tag，RC intent 不能恢复 stable tag。没有 Release 时，recovery 可以在同一 tag@SHA identity 下继续构建并创建 Release；已有 Release 时必须下载并验证既有 manifest 与 artifact digests，完整资产直接复用，带有 durable manifest 的部分资产只允许重建并上传缺失项，绝不覆盖已存在的资产。tag、Release 或部署已经部分成功时，重试只能继续这个 identity。

### Build and delivery

每种 artifact 只构建一次。firmware、devd、CLI、Web、bundle、manifest 和 Release 从同一个 runner `VERSION` 与 source SHA 生成；catalog 时间戳和压缩归档元数据也由 immutable source identity 决定，以保证同一 identity 的恢复 bytes 可重现。same-identity recovery 对完整 Release 只复用并验证既有资产，对带 durable manifest 的部分 Release 只重建缺失资产并要求 digest 与 manifest 完全一致，禁止覆盖已存在的资产；EdgeOne production/demo 复用已发布的 Web archive，不重新构建。部署开始前先写入不可变 `deployment-started` receipt，只有内容等于当前 source SHA 的 `deployed` marker 才能跳过部署；若只存在 started receipt，恢复必须 fail closed 并要求 operator review，避免 provider 已成功但 receipt 未上传时重复部署。Release、Deploy、Notify 分层，通知成功不等于 Release 或 Deploy 成功。

纯文档变更由 workflow 输出 skip，不能创建产品 Release；产品变更或文档与产品混合变更必须走完整 Release。workflow 失败时通知包含 version、merge/source SHA、reservation identity、tag（若已保留）和 same-identity recovery target。

## Agent 与 owner 合同

1. Agent 先读取 trusted baseline、当前 `main` exact SHA 和变更路径。
2. Agent 根据变更范围提出 `release_target` 建议，并同时给出按当前 baseline 解析的预期版本；这只是对话建议。
3. owner 确认 intent 后，owner 或 Agent 才能调用 manual dispatch；调用时只传 `release_target` 和 `operation`。
4. workflow 重新读取 baseline、tags 和 source SHA，独立解析 canonical version；workflow 不信任 Agent 的预期版本，也不从 labels、PR 数量、merge 顺序或 package metadata 推导版本。

## 验收标准

- 静态 contract tests 能证明新发布路径不读取 PR labels、`Label Gate`、`release_snapshot` 或 PR-local VERSION preparation。
- workflow dispatch 只接受四种 intent，并拒绝 caller version override；版本 policy 覆盖 baseline、单调性、collision、RC 序号和 idempotent recovery。
- 文档 allowlist、非纯文档和混合提交判定有自动化测试。
- 所有产品版本 consumer 使用同一 canonical VERSION resolver。
- quality-gates、CI PR、CI Main、Release Product 和 failure notification 的声明不再包含旧 label/preparation/completion 路径。
- 合并前只运行本地静态/契约质量门禁；真实 GitHub Release、manual dispatch 和 EdgeOne production/demo 属于合并后的 empirical acceptance。

## 非目标

- 不重写历史 tag、GitHub Release 或资产。
- 不改变 firmware、Web、devd 或 control-plane 功能。
- 不引入新的 notifier、secret、provider 或生产权限。
- 不允许 Agent 未经 owner 确认触发 dispatch，也不把 Agent 的精确版本建议写成 release target。

## 关联文件

- [Release Product workflow](../../../.github/workflows/release.yml)
- [Manual release policy](../../../.github/scripts/manual_release.py)
- [Quality gates](../../../.github/quality-gates.json)
- [Version source](../version-source/SPEC.md)
- [Developer policy](../../guides/flux-purr-developer-policy.md)
