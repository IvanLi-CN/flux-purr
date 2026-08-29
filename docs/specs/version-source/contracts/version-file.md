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
- 普通发布版本也不从其他状态读取；它是稳定 `VERSION` 的 `nextPatch`，随后由 Release Commit 写回文件。

### Examples（示例）

| `VERSION` | build mode | Product version |
| --- | --- | --- |
| `0.22.0` | development at `abcdef0` | `0.22.1-dev.abcdef0` |
| `0.22.0` | ordinary release | `0.22.1` |
| `0.22.0-rc.1` | release | `0.22.0-rc.1` |

### 兼容性与迁移（Compatibility / migration）

- Version File migration starts from `0.22.0`, the published product baseline immediately preceding this contract.
- A missing or invalid file is a hard error after migration. Cargo/NPM package versions, Git tags, release snapshots and environment variables are not fallbacks.
- An explicit major, minor or RC release writes the exact valid text in a Release Commit. All consumers then parse the file exactly as they do for an ordinary release.
