# Manual version release delivery history

## Lifecycle

本主题取代 Flux Purr 旧的 PR label release、PR-local VERSION preparation 和 completion/recovery 入口。历史 tag、GitHub Release 与资产保持不可变，只作为审计资料。

## Migration boundary

- 新发布从当前 `main` exact SHA 开始，trusted baseline 优先使用其祖先路径上的最高稳定 final tag。
- 非纯文档变更默认发布；文档 allowlist 只在当前 workflow 中用于明确 skip。
- `rc` 是独立 release intent，序号从下一个稳定基线计算；RC 不推进 stable baseline。
- 旧 recovery 的 `commit_sha`、label snapshot 和 VERSION-only preparation 不再有新建或消费路径。
