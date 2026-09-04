# Flux Purr 单一产品版本源

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，主题局部演进见 `./HISTORY.md`，持久决策的完整取舍见关联 ADR。

## 背景 / 问题陈述

- 历史 root、firmware、`devd` 与 Web package manifest 曾保留 `0.1.0`；版本源实现后，package metadata 仍可保持该值，但任何产品构建、运行时、bundle、manifest 或发布流程都不再读取它，也不从 Git tag 回退。
- PR `type:*` / `channel:*` labels 仍负责发布意图；通过验证的意图会写入同一 PR 分支上的 VERSION-only 准备提交。它们不提供数字版本，数字版本只从产品源 `VERSION` 解析。
- 每个已验证产品 PR 必须有独立的产品版本和准备提交。合并多个产品变更再发布会拉长回滚距离，并使生产问题难以定位到一个明确的变更边界。

## 目标 / 非目标

### Goals

- 根目录 `VERSION` 成为 Flux Purr 唯一的产品版本源。
- 开发构建自动从不变的 `VERSION` 生成可区分的开发显示版本。
- 每个已通过完整 PR CI 的产品源提交各自形成一个准备提交、正常 `main` 合并提交、tag、资产集合和回滚边界。
- firmware、`devd`、CLI、Web build metadata、firmware bundle 与 release manifest 使用同一个版本解析规则。
- 准备提交不重复完整 CI；它只在已经验证的源提交之后写入 `VERSION`。发布资产从其正常 `main` 合并提交构建一次。
- 保留 `Validate PR labels` 作为合并前的必需门禁；准备提交的 trailers 固化已验证 intent，避免发布时重新读取可变 PR 标签。

### Non-goals

- 不在普通开发构建、运行、测试或本地 bundle 生成时改写 `VERSION`。
- 不从 Git tag、Cargo/NPM package version、workflow channel 或 manifest 读取产品版本；PR label 和准备提交 metadata 只能表达已冻结的发布意图，不得覆盖 `VERSION` 的数字内容。
- 不为已有的未分版本历史重写 `main`，也不伪造其逐提交发布记录。
- 不改变各 package 的 package-manager metadata 版本；它们不再承担产品版本含义。

## 范围（Scope）

### In scope

- 根目录 `VERSION`、共享版本解析器与开发/发布 build identity。
- firmware build metadata、USB identity、`devd` health、CLI `--version` 与 Web build metadata。
- firmware bundle、release manifest、Git tag、版本准备提交和 recovery 的版本绑定。
- `CI PR`、`CI Main`、`Release Product`、质量门禁与 GitHub ruleset 的发布顺序约束。
- 旧的版本计算脚本退役；`Label Gate`、准备提交 metadata 和失败通知继续作为发布流程的审计与恢复路径。

### Out of scope

- Firmware、Web 或 control-plane 协议的功能行为变更。
- 真机烧录、HIL 或已发布设备的版本回填。
- 将每一个历史提交重新发布成新的版本。

## Related ADRs

- [0003-version-file-is-the-product-version-source](../../adr/0003-version-file-is-the-product-version-source.md)
- [0005-pr-local-version-preparation](../../adr/0005-pr-local-version-preparation.md)
- [0006-tag-reservation-and-legacy-release-reconciliation](../../adr/0006-tag-reservation-and-legacy-release-reconciliation.md)

## 需求（Requirements）

### MUST

- 根目录必须存在 UTF-8 的 `VERSION`，其内容必须符合 [Version File contract](./contracts/version-file.md)。它是 build、run、bundle、manifest 与 release 的唯一产品版本输入；唯一迁移例外是首次建立已发布基线的文件创建。
- 普通开发模式必须只读 `VERSION`，并显示 `nextPatch(VERSION)-dev.<short-sha>`。`short-sha` 仅区分同一开发版本的源修订，不参与产品版本计算。
- 普通发布模式必须从 PR 源提交的 `VERSION` 计算 `nextPatch(VERSION)`，在该同一 PR 分支创建只修改 `VERSION` 的准备提交，并从文件重新读取精确版本。Git tag 必须是 `v` 加该文件内容，并指向正常合并后的 `main` 提交。
- 准备提交前必须对 `v<version>` 执行 tag reservation。名称已存在时准备必须失败，且失败前不得写入 `VERSION`、创建提交或推送 PR 分支；只有 recovery 明确证明既有 tag 精确指向目标 merged `main` SHA 时才可复用。
- 每个通过完整 PR CI 且 release intent 启用的产品源提交必须单独发布。准备提交必须在 PR 合并前完成；多个源提交不得共用一个产品版本。`type:docs`/`type:skip` 明确表示不发布产品资产。
- 准备提交必须以已验证源提交为唯一父提交，diff 只能包含 `VERSION`，并带 `Release-Source-SHA`、`Product-Version` 和冻结 label intent metadata。该 metadata 只用于顺序验证与审计，不是版本输入。
- 在启用 signed-commit ruleset 时，准备提交必须通过 GitHub 的 `createCommitOnBranch` mutation 创建，使 GitHub 自动签名并验证该提交；工作流必须在签名未被验证时失败关闭，不得引入发布私钥、口令或绕过规则。
- Release controller 必须从正常合并后的 `main` 提交构建、tag、发布并验证完整资产；它绝不向 `main` push。发布失败时不得压缩、重算或替换版本；recovery 只能继续同一个 main SHA。
- 每个 PR 必须恰好有一个 `type:*` 和一个 `channel:*` 标签；`Validate PR labels` 必须拒绝缺失、重复或未知的 release-intent 标签，并将结果冻结到对应 PR head。
- `Release Product` 只能消费带有效准备提交的 main 合并提交；`type:docs`/`type:skip` 跳过产品发布，`type:patch + channel:stable` 唯一允许自动写入 `nextPatch(VERSION)`，`type:minor`、`type:major` 或 `channel:rc` 必须等待受控 `exact` 操作写入精确 VERSION 文本。label、metadata 与 channel 不得计算或解析数字版本。
- 所有版本化产物必须由该 main 合并提交构建。firmware identity、firmware bundle identity、`devd` `/health`、`flux-purr-devd --version`、`flux-purr --version`、Web build metadata、release manifest 和资产文件名必须一致表达该版本；source SHA 必须指向该 main 合并提交。
- package manifest 的 `0.1.0` 或其他 package metadata、Git tag、workflow inputs 与既有 manifest 不得作为版本回退或版本覆盖；snapshot 中的 labels 不得写入或覆盖数字版本。
- `CI PR` 对产品源提交运行完整矩阵；准备提交只运行结构验证。`CI Main` 对其正常 merge 进行结构验证，`Release Product` 从该 merge 构建一次资产，并且仅从这些已验证发布资产部署 EdgeOne production 与 public demo。每个产品源提交仍只运行一次完整 CI、一次发布资产构建和一组发布部署。
- 发布失败时，后续产品准备必须保持关闭；recovery 只能以现有 main 合并提交为目标，重建该提交的资产或 Release，不得重新计算、改写或合并版本。
- `channel` 若仍被 firmware bundle、catalog 或 GitHub Release 使用，必须从 `VERSION` 派生：稳定 SemVer 为 `stable`，`-rc.N` 为 `rc`，开发 build 为 `local`。不存在独立 channel 输入。
- 非普通 patch 的 major、minor 或 RC 发布必须以受控准备提交中的精确 `VERSION` 文本表达。该一次性写入完成后，所有下游步骤仍只读取文件；不得从 label 或 tag 推断该值。

### SHOULD

- 版本解析器应提供 machine-readable 输出，供 Rust build script、Bash、Python 与 Vite 共用，避免在多处复制 SemVer、next-patch 或 channel 推导逻辑。
- 准备提交 message 应使用 `chore(release): vX.Y.Z`，并记录其 source commit SHA 与冻结 intent；版本正确性始终由文件与 diff 验证，而不是提交消息。
- Release preparation 应使用现有 workflow `GITHUB_TOKEN` 的 `contents: write` 权限，只写已有 PR 分支以及 release tags/assets；不得为了发布流程增加 App、secret、variable、GitHub Environment 或 `main` bypass。

### COULD

- Web 可以在非侵入性的 build-info surface 显示当前产品版本和 source SHA，供支持与问题定位读取。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

#### 开发构建

1. 解析 root `VERSION`，不修改该文件。
2. 读取当前 commit 的短 SHA 作为 build qualifier。
3. 对稳定 `VERSION=0.23.0`，所有开发产物显示 `0.23.1-dev.<short-sha>`。
4. 生成的版本进入 firmware identity、local firmware bundle、`devd` health/CLI 和 Web build metadata；它不写回源码树。

#### 普通发布

1. 产品 PR 的源提交完成完整 PR CI 和 `Validate PR labels`。
2. `Prepare product version` 从受信任的 `main` policy checkout 检查当前 PR head、base 和 checks。自动 patch 写入 `nextPatch(VERSION)`；major、minor 或 RC intent 只接受受控 `exact` 写入精确 VERSION 文本。
3. 工作流在同一 PR 分支追加以源提交为父、且只修改 `VERSION` 的准备提交；其 trailers 固化 source SHA、产品版本与 validated intent。
4. `Release completion` 验证准备提交、当前 base 与 labels，并只读核验准备提交父提交的完整 CI 结果。PR 以 merge commit 正常合入 `main`，该 merge 的 tree 必须等于准备提交的 tree。
5. `CI Main` 结构验证该 merge；`Release Product` 只在其第二父提交是带完整 release trailers 的准备提交时继续。它从该 merge checkout，重新解析 `VERSION`，构建 Web、public demo、firmware 与 host-tools，生成、发布并校验资产、manifest 和 `vX.Y.Z` tag，然后从这些归档各部署一次 EdgeOne production 与 public demo。

#### Recovery

1. operator 指定一个已合入的产品 main SHA。
2. workflow 验证该 merge、其准备提交、tag 与 `VERSION` 一致性。
3. workflow 从该 main SHA 重建缺失资产或 GitHub Release。不得重算版本、改写 `VERSION`、移动 tag 或重新读取可变 PR 标签。

#### 预发布与显式版本变更

1. RC 的 `VERSION` 直接使用 `X.Y.Z-rc.N`，bundle/catalog channel 由该文本派生为 `rc`。
2. 稳定提升必须通过一个具有独立受保护 merge 边界的 prepared stable product PR 写入 `X.Y.Z`；不得 retag 原 RC commit。
3. major/minor 也遵循同一原则：精确版本在准备提交中被记录一次，随后只从文件读取。

### Edge cases / errors

- `VERSION` 缺失、空白、包含额外行或不符合 contract 时，所有产品构建必须失败；不得降级到 `0.1.0`、package manifest 或最近 tag。
- 源 PR 在准备后不再基于当前 `main` 时，`Release completion` 必须失败，直到重新验证并准备；它不得把多个源提交压缩到同一版本。
- 准备提交的 diff 不止 `VERSION`、父提交不是目标源提交、自动 patch 转换不符合普通规则、exact 文本非法或不严格高于当前 VERSION、标签 metadata 不匹配、main merge tree 不等于准备提交、或已有 tag 指向其他 commit 时，发布必须失败。
- 准备提交只运行结构校验；它不得触发第二次完整 CI 或产品资产构建。迁移完成后，人为 PR 修改 `VERSION` 必须被 release-completion gate 拒绝。
- release artifact 或 GitHub Release 只部分完成时，recovery 必须验证已存在 tag、manifest、资产哈希与 main merge identity，再决定复用或失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| root `VERSION` | file format | internal | New | [./contracts/version-file.md](./contracts/version-file.md) | release controller | build scripts, firmware, devd, CLI, Web, manifest | 唯一产品版本源 |
| product build identity | generated build metadata | internal | Modify | None | build tooling | firmware identity, devd health, CLI, Web | 同一 resolver 输出 |
| product release manifest | file format | external | Modify | [../../interfaces/http-api.md](../../interfaces/http-api.md) | release controller | Web, CLI, operators | `sourceSha` 绑定 main merge |

### 契约文档（按 Kind 拆分）

- [Version File](./contracts/version-file.md)

## 验收标准（Acceptance Criteria）

- Given `VERSION` contains `0.23.0` and source SHA is `abcdef0...`,
  When a development build runs,
  Then every product build identity reports `0.23.1-dev.abcdef0` and `VERSION` is byte-for-byte unchanged.

- Given a verified product PR source whose base `VERSION` is `0.23.0`,
  When version preparation runs,
  Then it appends exactly one child preparation commit with `VERSION=0.23.1`; after the normal merge, it builds and verifies assets from that merge and tags it `v0.23.1`.

- Given product merge A has a failed release,
  When recovery runs,
  Then it can only republish A's committed VERSION and source SHA; it cannot create a successor version.

- Given `VERSION` is malformed or absent,
  When firmware, `devd`, CLI, local firmware bundle, Web build, or release workflow runs,
  Then the command fails without consulting a tag or package manifest.

- Given a released version from main merge M,
  When firmware identity, `devd /health`, CLI `--version`, Web build metadata, firmware bundle identity and release manifest are inspected,
  Then their product version and M's SHA agree.

- Given a partial product release for main merge M,
  When recovery targets M,
  Then it republishes only M's identity when an existing tag points exactly at M, and fails on a foreign tag or manifest mismatch.

## 验收清单（Acceptance checklist）

- [x] 核心路径的长期行为已被明确描述。
- [x] 关键边界/错误场景已被覆盖。
- [x] 涉及的接口/契约已写清楚或明确为 `None`。
- [x] 相关验收条件已经可以用于实现与 review 对齐。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: resolver 覆盖稳定、RC、开发显示、普通 patch、非法 `VERSION`、无 tag/package fallback 与 channel derivation。
- Integration tests: firmware identity、`devd /health`、两个 CLI `--version`、local firmware bundle、Web build metadata 和 manifest 使用同一 resolver 输出。
- Workflow tests: 验证 Label Gate、准备提交 parent/diff/intent、准备提交父提交的完整 CI、source-to-release 一对一顺序、release-completion gate、CI 去重、recovery identity 和版本只读取 `VERSION`。

### UI / Storybook (if applicable)

- Stories to add/update: None unless Web exposes a visible build-info surface.
- Docs pages / state galleries to add/update: None.
- `play` / interaction coverage to add/update: None.
- Visual regression baseline changes (if any): None.

### Quality checks

- `cargo fmt --manifest-path firmware/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path firmware/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`
- `bun run --cwd web check`
- `bun run --cwd web typecheck`
- `bun run --cwd web build`
- dedicated version-resolver and workflow-static tests
- `git diff --check`

## Visual Evidence

None.

## Related PRs

- None

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`main` 必须将产品 PR 以 merge commit 合入，且 `Release completion` 必须列为远端 required check；否则 workflow 无法证明受保护 merge 保留了准备提交。此配置不授予 bypass 或新增身份。
- 风险：发布失败后 VERSION 已在 `main` 锁定。recovery 失败时不得准备下一产品版本；不得用合并多个提交来绕过。
- 假设：`type:docs` 和 `type:skip` 明确表示不生成产品版本；其余产品 PR 各自获得独立版本边界。
- 迁移边界：根 `VERSION=0.23.0`，历史 `v0.23.0` tag/release 保持不可变且仅作审计记录；首个完整新链路发布是一次正常 patch PR 的 `0.23.1`，不得追溯重发 `0.23.0`。

## 参考（References）

- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [ADR 0005](../../adr/0005-pr-local-version-preparation.md)
- [旧 PR label 发布规格](../pr-label-release-protection/SPEC.md)
- [CI Main](../../../.github/workflows/ci-main.yml)
- [Prepare product version](../../../.github/workflows/release-preparation.yml)
- [Release Product](../../../.github/workflows/release.yml)
- [GitHub repository ruleset REST contract](https://docs.github.com/en/rest/repos/rules?apiVersion=2026-03-10)
- [GitHub ruleset available rules](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)
