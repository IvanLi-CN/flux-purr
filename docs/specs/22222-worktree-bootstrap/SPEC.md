# Flux Purr Worktree Bootstrap（#22222）

## 状态

- Status: 已完成
- Owner: Flux Purr maintainers
- Scope: shared Git hooks、linked worktree bootstrap、repo-managed 开发依赖、bootstrap smoke gate

## 背景 / 问题陈述

Flux Purr 当前只在 README 中提供手动 `bun install` / `bun install --cwd web` / `bun run hooks:install` 路径。linked worktree 创建后不会自动补齐 `commitlint`、`lefthook`、web toolchain 或 Cargo fetch 预热，导致新 worktree 进入开发状态依赖维护者记忆。

仓库已经具备 `quality-gates`、PR label release 和多 surface CI，但“新 worktree 能否恢复到可开发状态”尚未被声明为正式 contract，也没有 smoke gate 防漂移。

## 目标 / 非目标

### Goals

- 建立 repo-local worktree bootstrap contract，让新 linked worktree 首次 checkout 后自动尝试恢复 repo-managed 开发依赖。
- 用 shared Git hooks 统一 `pre-commit`、`commit-msg`、`pre-push`、`post-checkout`，避免 per-worktree hooks 漂移。
- 提供显式 recovery 入口：`bun run bootstrap:dev` / `bun run worktree:setup`。
- 把 worktree bootstrap smoke 接入 CI PR / CI Main，并加入 required checks。
- 保持 odd Git layouts、custom `core.hooksPath`、历史 revision 缺少 bootstrap 脚本等场景的安全退化。

### Non-goals

- 不自动安装或修改系统级前置：`bun`、`rustup`、`espup`、`jq`、Playwright browsers。
- 不引入 `.env.local`、secrets 或其它本地资源 copy-missing manifest。
- 不改变 firmware / web / devd runtime contract、HIL 边界或 release artifact 行为。
- 不让 `post-checkout` 失败阻断 `git worktree add` 或 `git checkout`。

## 范围（Scope）

### In scope

- `scripts/install-hooks.sh`
- `scripts/run-lefthook-hook.sh`
- `scripts/post-checkout-bootstrap.sh`
- `scripts/bootstrap-dev.sh`
- `scripts/test-worktree-bootstrap.sh`
- 根目录 `package.json` script surface
- `lefthook.yml` 的 `post-checkout` wiring
- `CI PR` / `CI Main` 的 `Worktree bootstrap` job
- `.github/quality-gates.json`
- README 与开发者规范

### Out of scope

- GitHub UI branch protection 远端写入。
- Bun/Rust/Xtensa/Playwright 的系统安装器。
- 运行期端口租约、真实硬件烧录或设备配置同步。

## 需求（Requirements）

### MUST

- shared Git hooks 目录必须由 repo-local `scripts/install-hooks.sh` 管理，且包含 `pre-commit`、`commit-msg`、`pre-push`、`post-checkout` wrapper。
- `post-checkout` 必须调用当前 checkout 的 bootstrap runner；若当前 revision 缺少 runner，必须安全 no-op。
- 自动 bootstrap 只允许安装 repo-managed 开发依赖：根目录 `bun install --frozen-lockfile`、`web/` `bun install --frozen-lockfile`、Cargo fetch 预热、hooks 收口。
- Cargo fetch 预热属于 best-effort repo-managed 恢复层；它必须在临时 workspace snapshot 中执行，避免在真实 checkout 写入 bootstrap 生成的 `Cargo.lock`；若 Cargo 网络或 Xtensa toolchain 不健康，bootstrap 只输出 warning 和修复命令，不阻断 checkout。
- 系统前置缺失时，auto / manual 两条路径都必须只输出明确修复命令，不得尝试修改开发机，也不得让 checkout 失败。
- auto mode 必须在 linked worktree 首次 checkout 或 lock/manifests 变化时重跑对应层；未变化时只输出 skip/healthy 摘要。
- smoke test 必须覆盖真实 linked worktree、shared hooks、重复 checkout skip、历史 revision no-op、custom hook preservation。
- `Worktree bootstrap` 必须进入 PR required checks。

### SHOULD

- hook wrapper 应优先使用当前 worktree 可用的 `lefthook` binary，并可回退到主工作区 binary。
- bootstrap 状态摘要应按 root Bun / web Bun / Cargo / system prereq 分层输出，便于恢复。
- linked worktree 自动 bootstrap 不应在主工作区普通 branch checkout 上反复做无意义全量安装。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- 开发者在主工作区首次执行 `bun run hooks:install` 或 `bun run bootstrap:dev` 后，shared hooks 被安装到 common git dir。
- 新 linked worktree 在 `git worktree add` 触发的首次 checkout 中，通过 shared `post-checkout` hook 自动执行 `scripts/bootstrap-dev.sh --auto`。
- auto bootstrap 检查 root lockfile、`web/` lockfile、Cargo manifests 指纹；只重跑变化层。
- manual bootstrap 通过 `bun run bootstrap:dev` 或 `bun run worktree:setup` 触发，始终尝试完整 repo-managed 恢复路径。
- `pre-commit` / `commit-msg` / `pre-push` 通过 shared wrapper 回到当前 checkout 的 `lefthook.yml` 执行。

### Edge cases / errors

- 当前 checkout 缺少 `scripts/bootstrap-dev.sh`、`scripts/run-lefthook-hook.sh` 或 `scripts/post-checkout-bootstrap.sh` 时，shared hooks 必须 no-op，而不是报错中断 checkout。
- 当前 worktree 的 `node_modules` 缺失但主工作区有可用 `lefthook` binary 时，hook wrapper 应回退运行。
- `core.hooksPath` 指向自定义目录时，shared hooks 仍应保持由 common git dir 管理；不得覆盖自定义目录中已有的 owner hook。
- linked worktree 删除、原安装 worktree 删除、或历史 revision 切换都不得使剩余 worktree 的 shared hooks 失效。
- Playwright browser cache 缺失时，只提示 `cd web && bunx playwright install chromium`；不在 hook 中自动下载。

## 接口契约（Interfaces & Contracts）

- `bun run bootstrap:dev`
  - 完整恢复 repo-managed 开发依赖；warning-only。
- `bun run worktree:setup`
  - `bootstrap:dev` 的同义入口。
- `bun run test:worktree-bootstrap`
  - linked worktree bootstrap smoke gate。
- `scripts/bootstrap-dev.sh --auto --previous-ref <sha> --next-ref <sha> --branch-flag <0|1>`
  - 供 `post-checkout` 调用；warning-only。
- `scripts/bootstrap-dev.sh --manual`
  - 供显式恢复入口调用；warning-only。

## 验收标准

- Given 已 seed 的主工作区，When 创建新 linked worktree，Then 新 worktree 会自动补齐根目录与 `web/` repo-managed 依赖，并完成 Cargo fetch 预热。
- Given 同一 linked worktree 再次 checkout 且 lock/manifests 未变化，When `post-checkout` 触发，Then bootstrap 只输出 skip/healthy 摘要，不重复全量安装。
- Given 缺失 `bun`、`rustup`、`cargo +esp`、`jq` 或 Playwright browser cache，When auto 或 manual bootstrap 运行，Then 只输出明确修复命令且 checkout 不失败。
- Given shared hooks 已安装，When 删除原执行安装的 worktree 或切回旧 revision 缺少 bootstrap 脚本，Then 其它 worktree 的 shared hook wrapper 仍安全可用。
- Given `bun run test:worktree-bootstrap`，When 在临时 fixture repo 中执行，Then 它通过并证明 CI 可验证 linked worktree 恢复能力。

## 非功能性验收 / 质量门槛

- `bash scripts/test-worktree-bootstrap.sh`
- `python3 .github/scripts/check-quality-gates.py`
- `bash scripts/check-web-check.sh`
- `bash scripts/check-devd.sh`

## 文档更新

- `README.md`
- `docs/guides/flux-purr-developer-policy.md`
- `docs/specs/README.md`

## 实现里程碑

- [x] M1: shared Git hooks 入口与 wrapper 落地。
- [x] M2: auto/manual bootstrap 脚本落地并覆盖 root/web/Cargo。
- [x] M3: smoke test + CI gate + quality-gates 声明落地。
- [x] M4: README / developer policy / spec index 对齐当前事实。

## 风险与开放问题

- `post-checkout` 自动安装 repo-managed 依赖偏离 style-playbook 默认“轻量 no-install”口味，因此必须用 warning-only 与 smoke gate 控制风险。
- shared hook wrapper 需要兼容 detached worktree、historical revision 和 stale `lefthook` binary；没有真实 smoke test 时容易漂移。

## 假设

- Flux Purr 继续使用 Bun 作为 repo root 与 web 的 JS package manager。
- linked worktree 自动 bootstrap 允许访问网络安装 repo-managed 依赖；若离线，则按 warning-only 降级。
- Windows 不在首版自动 bootstrap 支持范围内。
