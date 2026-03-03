# PD Mini加热台二开资料采集与基础文档（#744yg）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- 当前仓库缺少针对「PD协议高颜值 mini 加热台」二次开发的统一资料基线。
- 外部信息分散在原版工程、二改工程与评论区，容易出现“口口相传但不可追溯”的决策风险。
- 若不先沉淀基础文档，后续硬件改板、固件迁移、兼容性验证会反复返工。

## 目标 / 非目标

### Goals

- 在仓库内落地一套可追溯的资料采集文档，覆盖原版与铜柱二改版。
- 固化关键事实：设计入口、附件可用性、装配/烧录/故障线索、许可证与合规边界。
- 输出二开风险与机会清单，支持后续实现阶段的决策。

### Non-goals

- 不改动 firmware/web 功能代码。
- 不执行硬件实物打样、烧录和实测。
- 不对外链商品参数、价格、库存做真实性背书。

## 范围（Scope）

### In scope

- 新增目录：`docs/research/mini-hotplate/`。
- 产出 8 份文档（README + 7 个专题文档）。
- 更新 `docs/specs/README.md` 索引。
- 更新仓库 `README.md` 的文档入口。

### Out of scope

- 登录后私有页面、受限下载链接的实际可获取性验证。
- 评论区完整翻页爬取。
- 任何商业化可用性承诺。

## 需求（Requirements）

### MUST

- 每个关键结论必须附来源 URL 与采集日期。
- 必须使用附件状态三态：`存在 / 不存在 / 未检查`。
- 必须覆盖两大主源（原版 + 铜柱版）。
- 必须包含二开风险与机会章节。
- 必须落地不可变证据清单（采集时间 + HTTP 状态 + 内容哈希）。

### SHOULD

- 对评论区信息按“作者信息/社区线索”分级，避免当作官方结论。
- 明确区分“已验证事实”和“待验证假设”。

### COULD

- 后续新增图像证据与 BOM 差异机读数据。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 研发成员从文档索引进入资料目录，先读 `README.md` 获取阅读顺序。
- 通过 `01-source-catalog.md` 快速确认来源可信度、更新时间与附件状态。
- 通过 `03/04/05/07` 文档完成“差异识别 -> Bring-up 排障 -> 二开决策”。

### Edge cases / errors

- 当来源信息冲突时，优先记录冲突并标注 `未检查`，不做臆断结论。
- 当附件无法直接验证下载时，只记录页面可见元数据并标记 `未检查`。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

None

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given 文档目录已创建，When 检查 `docs/research/mini-hotplate/`，Then 必须存在 8 个约定文件。
- Given 读取 `01-source-catalog.md`，When 查看主源条目，Then 每个主源均包含 URL、采集日期、更新时间、许可证、附件状态三态。
- Given 读取 `03-variant-delta-kinglf-vs-original.md`，When 检查核心差异，Then 必须出现“BOM/固件沿用、PCB重打、铜柱+铁氟龙替代探针”等条目并附来源。
- Given 读取 `05-assembly-flash-troubleshooting.md`，When 检查排障章节，Then 必须覆盖烧录接口、电压、温度 000、PD 诱骗、绝缘短路风险。
- Given 读取 `06-license-and-compliance-notes.md`，When 检查合规说明，Then 必须覆盖 GPL 3.0 与平台非商用/侵权投诉边界。
- Given 仓库与规格索引，When 查看入口，Then `README.md` 与 `docs/specs/README.md` 均能跳转到资料目录。
- Given 读取证据目录，When 检查 `docs/research/mini-hotplate/evidence/`，Then 必须存在可复核的 manifest 与哈希记录。

## 实现前置条件（Definition of Ready / Preconditions）

- `fast-track` 流程已由主人明确触发。
- 主源链接已明确（原版 + 铜柱版）。
- 文档范围限定为 docs-only。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: N/A（docs-only）
- Integration tests: N/A（docs-only）
- E2E tests (if applicable): N/A（docs-only）

### UI / Storybook (if applicable)

- N/A

### Quality checks

- `rg -n "TODO|TBD|待补充" docs/research/mini-hotplate`
- `rg -n "https?://" docs/research/mini-hotplate`
- `git diff -- docs README.md`

## 文档更新（Docs to Update）

- `README.md`: 增加 mini 加热台资料目录入口。
- `docs/specs/README.md`: 新增本规格索引记录。
- `docs/research/mini-hotplate/*`: 新增基础资料文档。
- `docs/research/mini-hotplate/evidence/*`: 新增采集证据与哈希清单。

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 创建 spec 并冻结资料采集口径
- [x] M2: 完成来源目录与参考设计摘要
- [x] M3: 完成差异分析、行为映射、排障与合规文档
- [x] M4: 完成二开风险机会文档并接入仓库入口

## 方案概述（Approach, high-level）

- 采用“来源目录 -> 主体分析 -> 风险决策”的三层结构。
- 只保留可溯源事实，社区经验以“线索”标记，不替代官方说明。
- 用三态标记覆盖“已知未知”，避免默认省略导致误判。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：评论区结论可靠性不一，可能存在个体焊接差异导致的噪声。
- 开放问题：附件文件真实内容与版本差异尚未解包核对。
- 假设：后续实现阶段会先做小样验证，再决定固件/硬件重构路线。

## 变更记录（Change log）

- 2026-03-03: 创建规格并完成 docs-only 基线交付。
- 2026-03-03: 按 review 反馈补充不可变证据清单与索引直达入口。
- 2026-03-03: 按 review 反馈补齐 artifact 快照路径与哈希，并修复螺丝规格 Markdown 误解析风险。
- 2026-03-03: 按 review 反馈将 evidence artifact 收敛为结构化摘录（去除追踪参数 + token/账号标识/页面 ID 脱敏）并回写 manifest/README 哈希。
- 2026-03-03: 按 review 反馈下调“评论区”相关结论为待验证假设，并将评论可见性统一为三态未检查（登录态受限）。

## 参考（References）

- https://oshwhub.com/littleoandlittleq/bian-xie-jia-re-tai
- https://oshwhub.com/kinglf/pd-xie-yi-gao-yan-zhi-mini-jia-re-tai
