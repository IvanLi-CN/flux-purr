# Flux Purr 单一产品版本源

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，主题局部演进见 `./HISTORY.md`，持久决策的完整取舍见关联 ADR。

## 背景 / 问题陈述

- 历史 root、firmware、`devd` 与 Web package manifest 曾保留 `0.1.0`；版本源实现后，package metadata 仍可保持该值，但任何产品构建、运行时、bundle、manifest 或发布流程都不再读取它，也不从 Git tag 回退。
- PR `type:*` / `channel:*` labels 与 Git notes snapshot 仍负责冻结发布意图和发布顺序；它们不提供数字版本，数字版本只从产品源 `VERSION` 解析。
- 每个已验证产品源提交必须有独立的产品版本和 Release Commit。合并多个提交再发布会拉长回滚距离，并使生产问题难以定位到一个明确的变更边界。

## 目标 / 非目标

### Goals

- 根目录 `VERSION` 成为 Flux Purr 唯一的产品版本源。
- 开发构建自动从不变的 `VERSION` 生成可区分的开发显示版本。
- 每个已通过 `CI Main` 的产品源提交各自形成一个 Release Commit、tag、资产集合和回滚边界。
- firmware、`devd`、CLI、Web build metadata、firmware bundle 与 release manifest 使用同一个版本解析规则。
- Release Commit 不重复完整 CI；它只在已经验证的源提交之后写入 `VERSION` 并触发对应的发布资产构建。
- 保留 `Validate PR labels` 作为合并前的必需门禁，并在主线上持久化已验证的 release intent snapshot，避免发布时重新读取可变 PR 标签。

### Non-goals

- 不在普通开发构建、运行、测试或本地 bundle 生成时改写 `VERSION`。
- 不从 Git tag、Cargo/NPM package version、workflow channel 或 manifest 读取产品版本；PR label 和 Git notes 只能表达已冻结的发布意图，不得覆盖 `VERSION` 的数字内容。
- 不为已有的未分版本历史重写 `main`，也不伪造其逐提交发布记录。
- 不改变各 package 的 package-manager metadata 版本；它们不再承担产品版本含义。

## 范围（Scope）

### In scope

- 根目录 `VERSION`、共享版本解析器与开发/发布 build identity。
- firmware build metadata、USB identity、`devd` health、CLI `--version` 与 Web build metadata。
- firmware bundle、release manifest、Git tag、Release Commit 和 recovery 的版本绑定。
- `CI PR`、`CI Main`、`Release Product`、质量门禁与 GitHub ruleset 的发布顺序约束。
- 旧的版本计算脚本退役；`Label Gate`、release intent snapshot、queue/backfill 和失败通知继续作为发布流程的审计与补偿路径。

### Out of scope

- Firmware、Web 或 control-plane 协议的功能行为变更。
- 真机烧录、HIL 或已发布设备的版本回填。
- 将每一个历史提交重新发布成新的版本。

## Related ADRs

- [0003-version-file-is-the-product-version-source](../../adr/0003-version-file-is-the-product-version-source.md)

## 需求（Requirements）

### MUST

- 根目录必须存在 UTF-8 的 `VERSION`，其内容必须符合 [Version File contract](./contracts/version-file.md)。它是 build、run、bundle、manifest 与 release 的唯一产品版本输入；唯一迁移例外是首次建立已发布基线的文件创建。
- 普通开发模式必须只读 `VERSION`，并显示 `nextPatch(VERSION)-dev.<short-sha>`。`short-sha` 仅区分同一开发版本的源修订，不参与产品版本计算。
- 普通发布模式必须从源提交的 `VERSION` 计算 `nextPatch(VERSION)`，创建只修改 `VERSION` 的 Release Commit，并从该 Release Commit 重新读取精确版本。Git tag 必须是 `v` 加该文件内容，并指向 Release Commit。
- 每个通过 `CI Main` 且 release intent 启用的产品源提交必须单独发布。Release Commit、tag 和资产完成前，下一产品源提交不得合入 `main`；多个源提交不得共用一个产品版本。`type:docs`/`type:skip` 明确表示不发布产品资产。
- Release Commit 必须以已验证源提交为唯一父提交，diff 只能包含 `VERSION`，并带可审计的 source-commit metadata。该 metadata 只用于顺序验证与审计，不是版本输入。
- Release controller 必须先从 Release Commit 构建、tag、发布并验证完整资产，再将 `main` fast-forward 到该 commit。`main` 推进失败时不得压缩、重算或替换版本；recovery 只能继续同一 Release Commit。
- 每个 PR 必须恰好有一个 `type:*` 和一个 `channel:*` 标签；`Validate PR labels` 必须拒绝缺失、重复或未知的 release-intent 标签，并将结果冻结到对应 PR head。
- `Release Product` 只能消费主线上与源提交绑定的不可变 snapshot；`type:docs`/`type:skip` 跳过产品发布，其他 type/channel 只选择 VERSION 的 bump/channel 动作。
- 所有版本化产物必须由 Release Commit 构建。firmware identity、firmware bundle identity、`devd` `/health`、`flux-purr-devd --version`、`flux-purr --version`、Web build metadata、release manifest 和资产文件名必须一致表达该版本；source SHA 必须指向 Release Commit。
- package manifest 的 `0.1.0` 或其他 package metadata、Git tag、workflow inputs 与既有 manifest 不得作为版本回退或版本覆盖；snapshot 中的 labels 不得写入或覆盖数字版本。
- `CI Main` 必须跳过有效 Release Commit 的完整矩阵；`Release Product` 也必须忽略该 push，避免递归 CI 或二次发布。每个产品源提交仍只运行一次完整 CI 和一次发布资产构建。
- 发布失败时，release-completion gate 必须保持关闭；recovery 只能以现有 Release Commit 为目标，重建该提交的资产或 Release，不得重新计算、改写或合并版本。
- `channel` 若仍被 firmware bundle、catalog 或 GitHub Release 使用，必须从 `VERSION` 派生：稳定 SemVer 为 `stable`，`-rc.N` 为 `rc`，开发 build 为 `local`。不存在独立 channel 输入。
- 非普通 patch 的 major、minor 或 RC 发布必须以受控 Release Commit 中的精确 `VERSION` 文本表达。该一次性写入完成后，所有下游步骤仍只读取文件；不得从 label 或 tag 推断该值。

### SHOULD

- 版本解析器应提供 machine-readable 输出，供 Rust build script、Bash、Python 与 Vite 共用，避免在多处复制 SemVer、next-patch 或 channel 推导逻辑。
- Release Commit message 应使用 `chore(release): vX.Y.Z`，并记录其 source commit SHA；版本正确性始终由文件与 diff 验证，而不是提交消息。
- Release controller 应使用现有 workflow token，并由仓库 ruleset 明确允许该 workflow identity 执行受保护 Release Commit 的 fast-forward；不得为了发布流程凭空增加 GitHub environment。

### COULD

- Web 可以在非侵入性的 build-info surface 显示当前产品版本和 source SHA，供支持与问题定位读取。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

#### 开发构建

1. 解析 root `VERSION`，不修改该文件。
2. 读取当前 commit 的短 SHA 作为 build qualifier。
3. 对稳定 `VERSION=0.22.0`，所有开发产物显示 `0.22.1-dev.<short-sha>`。
4. 生成的版本进入 firmware identity、local firmware bundle、`devd` health/CLI 和 Web build metadata；它不写回源码树。

#### 普通发布

1. 一个产品源提交进入 `main` 并完成 `CI Main`。
2. Release controller 为该源提交补齐并读取不可变 release intent snapshot，确认该提交仍是 `main` 头部、其父发布链完整且其 `VERSION` 有效。
3. controller 按 snapshot 的 release level/channel 从 `VERSION` 计算目标值，创建以该源提交为父、且只修改 `VERSION` 的 Release Commit，但暂不推进 `main`。
4. controller 从 Release Commit checkout，重新解析 `VERSION`，构建 Web、firmware 与 host-tools，生成、发布并校验 firmware bundle、manifest 和 `vX.Y.Z` tag。
5. 只有发布校验成功后，controller 才将 `main` fast-forward 到 Release Commit 并打开下一次产品源提交的合入门禁。

#### Recovery

1. operator 指定一个已有 Release Commit。
2. workflow 验证该 commit 的单文件 diff、tag 与 `VERSION` 一致性。
3. workflow 从该 commit 重建缺失资产或 GitHub Release；只有身份完整后才可将该 commit fast-forward 到 `main`。不得重算版本、改写 `VERSION`、移动 tag 或重新读取可变 PR 标签；recovery 使用同一个 release intent snapshot。

#### 预发布与显式版本变更

1. RC 的 `VERSION` 直接使用 `X.Y.Z-rc.N`，bundle/catalog channel 由该文本派生为 `rc`。
2. 将 RC 提升为稳定版本时，创建新的稳定 Release Commit，写入 `X.Y.Z`；不得 retag 原 RC commit。
3. major/minor 也遵循同一原则：精确版本在 Release Commit 中被记录一次，随后只从文件读取。

### Edge cases / errors

- `VERSION` 缺失、空白、包含额外行或不符合 contract 时，所有产品构建必须失败；不得降级到 `0.1.0`、package manifest 或最近 tag。
- 目标源提交在 CI 完成后不再是 `main` 头部，或 Release Commit 最终无法 fast-forward `main` 时，release controller 必须失败并保持 merge gate 关闭；它不得把多个源提交压缩到同一版本。
- Release Commit 的 diff 不止 `VERSION`、父提交不是目标源提交、版本转换不符合普通规则或已有 tag 指向其他 commit 时，发布必须失败。
- Release Commit push 不得触发完整 CI、Label Gate 或第二个 product release；迁移完成后，人为 PR 修改 `VERSION` 必须被 release-completion gate 拒绝。
- release artifact 或 GitHub Release 只部分完成时，recovery 必须验证已存在 tag、manifest、资产哈希与 Release Commit identity，再决定复用或失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| root `VERSION` | file format | internal | New | [./contracts/version-file.md](./contracts/version-file.md) | release controller | build scripts, firmware, devd, CLI, Web, manifest | 唯一产品版本源 |
| product build identity | generated build metadata | internal | Modify | None | build tooling | firmware identity, devd health, CLI, Web | 同一 resolver 输出 |
| product release manifest | file format | external | Modify | [../../interfaces/http-api.md](../../interfaces/http-api.md) | release controller | Web, CLI, operators | `sourceSha` 绑定 Release Commit |

### 契约文档（按 Kind 拆分）

- [Version File](./contracts/version-file.md)

## 验收标准（Acceptance Criteria）

- Given `VERSION` contains `0.22.0` and source SHA is `abcdef0...`,
  When a development build runs,
  Then every product build identity reports `0.22.1-dev.abcdef0` and `VERSION` is byte-for-byte unchanged.

- Given a verified product-source commit whose `VERSION` is `0.22.0`,
  When the ordinary release controller runs,
  Then it creates exactly one child Release Commit with `VERSION=0.22.1`, publishes and verifies assets from that child, tags it `v0.22.1`, and only then fast-forwards `main` to it.

- Given source commit A has a pending or failed release,
  When a PR for source commit B attempts to merge,
  Then the required release-completion check blocks the merge until A's Release Commit, tag, manifest and assets are complete and `main` points to that Release Commit.

- Given `VERSION` is malformed or absent,
  When firmware, `devd`, CLI, local firmware bundle, Web build, or release workflow runs,
  Then the command fails without consulting a tag or package manifest.

- Given a released version,
  When firmware identity, `devd /health`, CLI `--version`, Web build metadata, firmware bundle identity and release manifest are inspected,
  Then their product version and Release Commit SHA agree.

- Given a partial product release for Release Commit R,
  When recovery targets R,
  Then it republishes only R's identity and fails on any existing tag or manifest mismatch.

## 验收清单（Acceptance checklist）

- [x] 核心路径的长期行为已被明确描述。
- [x] 关键边界/错误场景已被覆盖。
- [x] 涉及的接口/契约已写清楚或明确为 `None`。
- [x] 相关验收条件已经可以用于实现与 review 对齐。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: resolver 覆盖稳定、RC、开发显示、普通 patch、非法 `VERSION`、无 tag/package fallback 与 channel derivation。
- Integration tests: firmware identity、`devd /health`、两个 CLI `--version`、local firmware bundle、Web build metadata 和 manifest 使用同一 resolver 输出。
- Workflow tests: 验证 Label Gate、intent snapshot、Release Commit parent/diff、source-to-release 一对一顺序、release-completion gate、release commit skip、recovery identity 和版本只读取 `VERSION`。

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

- 风险：active `main` ruleset 目前要求 PR、签名和严格 checks，且没有允许 release workflow identity 的 bypass；现有 `GITHUB_TOKEN` 因此不能创建受保护 Release Commit。仓库还同时保留 classic branch protection，GitHub 会同时执行两套规则。owner 必须先将 classic rule 的保护完整收敛到 ruleset 并移除 classic rule，再为现有 release workflow identity 配置最小必要的 bypass；发布流程不依赖额外 GitHub environment。
- 风险：release-completion gate 会在发布失败时停止下一次合入。它只在 `main` 头部是带已验证 tag、manifest 和资产的 Release Commit 时成功。这是保留逐提交回滚距离的必要代价，不得用合并多个提交来绕过。
- 假设：每个非 Release Commit 的 `main` 变更都代表一个产品版本；文档和维护类变更同样获得独立 patch release。
- 假设：迁移基线为现有已发布 `v0.22.0`。版本源实现 PR 一次性加入 `VERSION=0.22.0`，其随后 Release Commit 发布 `0.22.1`。在此之前已经积累但没有 Version File 的历史无法在不重写 `main` 的前提下重新切割。

## 参考（References）

- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [旧 PR label 发布规格](../r9k3m-pr-label-release-protection/SPEC.md)
- [CI Main](../../../.github/workflows/ci-main.yml)
- [Release Product](../../../.github/workflows/release.yml)
- [GitHub repository ruleset REST contract](https://docs.github.com/en/rest/repos/rules?apiVersion=2026-03-10)
- [GitHub ruleset available rules](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)
