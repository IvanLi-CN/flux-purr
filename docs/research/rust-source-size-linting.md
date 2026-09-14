# Rust 源码规模与风格约束

## 结论

Rust/Clippy 原生能可靠约束函数和方法的规模、参数量与部分复杂度，不能约束单个 `.rs` 文件的总行数。因此，超长源文件应使用两层规则：Clippy 管函数边界，仓库自有的确定性脚本管文件预算。不要把 `clippy::too_many_lines` 误当成 file-size lint。

当前仓库的 firmware 与 devd 检查脚本都显式运行 `rustfmt`/`clippy`，并将
`too_many_lines`、`too_many_arguments` 与 `excessive_nesting` 提升为拒绝级别。
`too_many_lines` 仍是 `pedantic` 且默认 `allow`，所以必须在命令行显式启用；
`excessive_nesting` 也必须通过 `clippy.toml` 配置阈值后才会产生诊断。

## Clippy 原生规则

| 规则 | 默认级别与阈值 | 适用边界 | 建议 |
| --- | --- | --- | --- |
| `clippy::too_many_lines` | `pedantic` / `allow`；100 行 | 函数或方法体 | 显式 `-D clippy::too_many_lines`，在测量后将阈值设为 80 或 100。它不限制文件总行数。 |
| `clippy::too_many_arguments` | `complexity` / `warn`；7 参数 | Rust ABI 函数、方法及 trait 声明；trait impl 和 `extern` 声明会豁免 | 保留默认阈值，作为 error 执行；以参数 struct 或领域对象替代不相关的位置参数。 |
| `clippy::cognitive_complexity` | `restriction` / `allow`；25 | 函数、方法与闭包 | 不作为硬性风格门禁。Clippy 明确说明它不能真正度量认知复杂度，并建议优先考虑 `too_many_lines`、`excessive_nesting`。`cyclomatic_complexity` 是其历史名称。 |

官方依据：

- [Clippy lint list: `too_many_lines`](https://rust-lang.github.io/rust-clippy/master/index.html#too_many_lines)
- [Clippy lint list: `too_many_arguments`](https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments)
- [Clippy lint list: `cognitive_complexity`](https://rust-lang.github.io/rust-clippy/master/index.html#cognitive_complexity)
- [Clippy 配置项与默认值](https://doc.rust-lang.org/clippy/lint_configuration.html)

## 配置位置与执行方式

阈值放在各 Rust crate 的 `clippy.toml`（或其可发现的上级目录）中；Clippy 会从 `CLIPPY_CONF_DIR`、`CARGO_MANIFEST_DIR`、当前目录起向上查找。两个独立 crate 应使用各自的文件，避免 firmware 与 native devd 被同一阈值意外绑定。

```toml
# <crate>/clippy.toml
too-many-lines-threshold = 100
too-many-arguments-threshold = 7
# 仅在采样期启用 cognitive_complexity 时才有意义。
cognitive-complexity-threshold = 25
```

启用层级有三种，按适用范围选择：

1. CI/检查脚本：`cargo clippy --all-targets -- -D warnings -D clippy::too_many_lines`。这是本项目首选，规则不侵入产品源文件。
2. crate attribute：`#![deny(clippy::too_many_lines)]`。规则随 crate 分发，但 firmware 与 devd 都要单独声明；局部例外使用带原因的 `#[expect(clippy::too_many_lines)]`，让例外在债务消失后自动失效。
3. `Cargo.toml` 的 `[lints.clippy]`。Cargo 支持为本 package 设 lint level；它不影响依赖。可替代 crate attribute，但不与脚本重复声明同一等级。

`-D warnings` 只会把已启用的 warning 升级为 error，不能启用默认 `allow` 的 `too_many_lines` 或 `cognitive_complexity`。`cargo clippy --all-targets` 会包含 lib、bin、tests、benches 与 examples。

官方依据：

- [Clippy 使用与命令行 lint level](https://doc.rust-lang.org/clippy/usage.html)
- [Clippy 配置文件查找规则](https://doc.rust-lang.org/clippy/configuration.html)
- [Cargo `[lints]` manifest section](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)
- [Rust lint level 的 attribute 与 CLI 优先级](https://doc.rust-lang.org/rustc/lints/levels.html)
- [Cargo `--all-targets` 的范围](https://doc.rust-lang.org/cargo/commands/cargo-check.html#target-selection)

## 测试与 rustfmt

保留 `--all-targets`：测试也是可维护代码。`too_many_lines` 与 `too_many_arguments` 没有通用的测试豁免；`cognitive_complexity` 会跳过直接标注 `#[test]` 的函数，但不应据此当作测试策略。确有必要的表格驱动测试或协议 fixture，应在最小函数范围写明原因的局部 `#[expect]`，不得对整个测试模块或 crate 放宽规则。

`rustfmt` 解决排版一致性，不解决模块或函数过大。继续使用稳定 toolchain 的 `cargo fmt --manifest-path <crate>/Cargo.toml --all -- --check`；若新增 `rustfmt.toml`，只采用标为 stable 的选项，避免 nightly-only 格式漂移。

官方依据：

- [cargo-fmt](https://doc.rust-lang.org/cargo/commands/cargo-fmt.html)
- [rustfmt 配置与 stable/unstable 区分](https://github.com/rust-lang/rustfmt/blob/master/Configurations.md)

## 安全分阶段落地

1. 在 firmware 与 devd 保持 `--all-targets`，先用报告模式确认触发函数，再逐项拆出命名明确的领域函数或参数 struct。
2. 使用各 crate 的 `clippy.toml`，由检查脚本统一执行 `cargo fmt --check` 和严格的
   `cargo clippy --all-targets`；`too_many_arguments` 随 `-D warnings` 变为 error，
   另外两项由命令行显式提升。
3. 结构性例外只能落在最小函数范围，并使用带原因的 `#[expect]`；仓库 checker
   拒绝 crate/file/module 级例外与任何本地 `too_many_arguments` 例外。新函数不得
   获得 blanket allow。
4. 另建不依赖 Clippy 的入口边界检查：只对三个装配入口统计 500 行预算并拒绝
   内联测试；领域模块不设置统一文件行数上限，按职责边界拆分。
