# Evidence Snapshot (2026-03-03)

本目录保存资料采集时的不可变证据（HTTP 状态 + 内容哈希 + 结构化证据摘录）。

- Manifest: `source-manifest-2026-03-03.json`
- Generated at (UTC): `2026-03-03T08:02:44.004129+00:00`

| Source | URL | HTTP | Content SHA256 | Bytes | Artifact | Artifact SHA256 | Processing | Fetched at (UTC) | Note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | https://oshwhub.com/littleoandlittleq/bian-xie-jia-re-tai | 200 | `88b3862917a3362ed64c5126226f789c8d8bd8add50c0f90a796c46c73882d31` | 520392 | `docs/research/mini-hotplate/evidence/artifacts/S1-oshwhub-original.html.txt` | `25661bee946c6925575ac19fc5cfa619bcffaa9598f8d399d9db97dd91ee2580` | structured extract + tracking param stripping + identifier redaction | 2026-03-03T07:37:21.814412+00:00 | - |
| S2 | https://oshwhub.com/kinglf/pd-xie-yi-gao-yan-zhi-mini-jia-re-tai | 200 | `1a06ac2c635fc662280692fd548b9c84bd146482fee7eaa0544b710c5dc000d0` | 499404 | `docs/research/mini-hotplate/evidence/artifacts/S2-oshwhub-kinglf.html.txt` | `509e5c9650f2ac1bbde5570b25096faac63b8a19da0c594aed93cf39b65ffb9b` | structured extract + tracking param stripping + identifier redaction | 2026-03-03T07:37:22.586898+00:00 | - |
| S3 | https://lceda.cn/page/appeal | 200 | `457df145ebe1670785676da99c034e8a6ab687f7836de4f86c4f05f490b91ed1` | 486829 | `docs/research/mini-hotplate/evidence/artifacts/S3-lceda-appeal.html.txt` | `32e347737efe8e1898f6e54c23c0ee914222f794136deaf241626ff804a737cd` | structured extract + tracking param stripping + identifier redaction | 2026-03-03T07:37:23.303948+00:00 | - |
| S4 | https://www.eeworld.com.cn/RDesigns_detail/79318 | 403 | `323c42818fa3b7324113258a660ab63d069fce5cc6667b4adbcc5d5b4fe4e3ad` | 1236 | `docs/research/mini-hotplate/evidence/artifacts/S4-eeworld.html.txt` | `76d78d857805a5d0f3d8649f867c13eedebe30446fc7e7d43a9f72e0e9b4c4ee` | structured extract + tracking param stripping + identifier redaction | 2026-03-03T07:37:23.771559+00:00 | HTTPError: Forbidden |

说明：
- `HTTP=403` 代表来源对当前抓取方式有限制，文档中应保持 `未检查` 或“线索”语义。
- `Content SHA256` 对应抓取时的原始响应字节哈希（原始响应不入库）。
- `Artifact` 仅保留结构化证据摘录，不保存完整第三方页面 HTML。
- 结构化摘录默认去除追踪参数（如 `_u`、`spm`）并脱敏账号标识/页面 ID。
- 哈希用于复核“是否同一版本证据文本”，不代表真实性背书。
