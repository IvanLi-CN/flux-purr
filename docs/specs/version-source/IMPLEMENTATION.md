# Flux Purr 单一产品版本源实现状态

## Current status

- Implementation: active.
- Product source: root `VERSION`。
- Shared parser: `scripts/product-version.py`。
- Manual release policy: `.github/scripts/manual_release.py`。
- Release workflow: `.github/workflows/release.yml`。

## Coverage

- firmware build metadata、`flux-purr-devd` build script、CLI、health、local bundle、Vite metadata、firmware catalog、release manifest 和 Release asset names 都由同一 VERSION resolver 派生。
- development mode 只读 VERSION，生成 `nextPatch(VERSION)-dev.<short-sha>`；release mode 读取 runner workspace 中由 manual policy 解析并绑定的 canonical VERSION。
- CI PR/CI Main 不再创建或验证 VERSION-only preparation commit，也不读取 PR labels。
- Manual Release 在构建前绑定 `main` exact SHA、trusted baseline、intent、canonical version 和 tag reservation；EdgeOne production/demo 复用同一已发布 Web archive。

## Verification

- `scripts/test-product-version.py` 覆盖严格 VERSION 解析、development read-only identity、RC channel 和无 fallback。
- `.github/scripts/test-manual-release.sh` 覆盖 patch/minor/major/rc、docs-only/mixed、collision 和 same-identity recovery。
- `.github/scripts/test-release-workflows.sh` 覆盖 workflow dispatch、旧路径删除、quality-gates 与通知契约。
