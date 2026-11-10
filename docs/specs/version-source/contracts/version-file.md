# Product Version File (`VERSION`)

## `VERSION` (root file)

- 范围（Scope）: internal
- 变更（Change）: New
- 编码（Encoding）: UTF-8, ASCII content, one trailing LF

### Schema（结构）

- 文件必须只有一行，不能有前后空白、空行、注释或 build metadata。
- 稳定版本格式为 `X.Y.Z`，其中 `X`、`Y`、`Z` 是无前导零的十进制整数，`X.Y.Z` 符合 SemVer 2.0.0 core version。
- RC 格式为 `X.Y.Z-rc.N`，其中 `N` 是正十进制整数。RC 是唯一被当前产品 release channel 支持的 prerelease form。
- 普通开发版本不写入文件；它由稳定或 RC numeric core 的 `nextPatch` 与当前 source SHA 生成：`X.Y.(Z+1)-dev.<short-sha>`。
- manual release 的 canonical version 由 owner-confirmed `release_target` 与 trusted stable baseline 解析；解析结果只在 release runner workspace 写入 VERSION。PR labels、PR-local VERSION preparation 和 caller-provided exact version 不属于活动输入。

### Examples（示例）

| `VERSION` | build mode | Product version |
| --- | --- | --- |
| `0.23.0` | development at `abcdef0` | `0.23.1-dev.abcdef0` |
| `0.23.0` | ordinary release | `0.23.1` |
| `0.23.0-rc.1` | release | `0.23.0-rc.1` |

### 兼容性与迁移（Compatibility / migration）

- Version File migration starts from `0.23.0`. The historical `v0.23.0` tag/release is preserved as audit history; the first complete new-chain release is `0.23.1`.
- A missing or invalid file is a hard error after migration. Cargo/NPM package versions, Git tags, release snapshots and environment variables are not fallbacks.
- An explicit major, minor or RC release is resolved by the manual Version Policy, then all consumers parse the runner workspace file exactly as they do for an ordinary release.
