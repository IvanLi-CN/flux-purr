# Flux Purr 单一产品版本源主题历史

## Lifecycle

根 `VERSION` 继续是产品版本事实；本主题的发布边界由 Manual version release delivery 接管。

## Replacements

- PR label、label snapshot、VERSION-only preparation 和 release completion 不再是新发布路径的一部分。
- 新发布从当前 `main` exact SHA 与 trusted stable baseline 解析 canonical version；历史 tag、Release、manifest 和资产保持不可变。
- `rc` 从下一个稳定基线计算 `rc.N`，不推进 stable baseline；same-identity recovery 不创建 successor。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- [Manual version release delivery](../manual-version-release-delivery/SPEC.md)
- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [ADR 0025](../../adr/0025-manual-version-release-delivery.md)
