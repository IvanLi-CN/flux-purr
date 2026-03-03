# PD Mini加热台二开资料基线

本目录沉淀「[PD协议 | 高颜值]mini加热台」原版与铜柱二改版的公开资料，目标是为后续二次开发提供可追溯、可决策的基础文档。

## 阅读顺序

1. `01-source-catalog.md`：来源清单与可信度边界
2. `02-reference-design-summary.md`：原版设计的硬件/软件基线
3. `03-variant-delta-kinglf-vs-original.md`：铜柱版与原版差异
4. `04-hardware-firmware-behavior-map.md`：行为映射与二开影响
5. `05-assembly-flash-troubleshooting.md`：组装/烧录/故障排查
6. `06-license-and-compliance-notes.md`：许可证与合规边界
7. `07-secondary-dev-risks-and-opportunities.md`：风险与机会清单

## 证据约定

- 每条关键结论都标注来源编号（如 `[S1]`）与 URL。
- 附件/下载类信息使用三态：`存在 / 不存在 / 未检查`。
- 评论区信息默认归类为“社区线索”，需要后续实测验证。

## 来源索引

- [S1] 原版项目（小O和小Q）
  URL: https://oshwhub.com/littleoandlittleq/bian-xie-jia-re-tai
  采集日期: 2026-03-03
- [S2] 铜柱二改项目（kinglf）
  URL: https://oshwhub.com/kinglf/pd-xie-yi-gao-yan-zhi-mini-jia-re-tai
  采集日期: 2026-03-03
- [S3] 侵权投诉与申诉规则（平台链接）
  URL: https://lceda.cn/page/appeal
  采集日期: 2026-03-03
- [S4] EEWorld 转载页（交叉参考）
  URL: https://www.eeworld.com.cn/RDesigns_detail/79318
  采集日期: 2026-03-03
  当前可达性: 访问受限（HTTP 403）

## 当前状态

- 文档基线：存在
- 不可变证据（hash manifest）：存在（`docs/research/mini-hotplate/evidence/`）
- 附件文件实物解包：未检查
- 登录后深层评论全量采集：未检查
