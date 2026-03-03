# 01 来源目录与采集边界

## 主源清单（用于决策）

| Source ID | 名称 | URL | 角色 | 创建时间 | 更新时间 | 开源协议 | 附件状态 | 评论可见状态 | 采集日期 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | 原版 mini 加热台（小O和小Q） | https://oshwhub.com/littleoandlittleq/bian-xie-jia-re-tai | 主事实源 | 2022-09-12 19:49:10 | 2023-07-27 09:58:31 | GPL 3.0 | 存在（zip/mp4/hex） | 存在（首屏 + 部分最新评论可见） | 2026-03-03 |
| S2 | 铜柱二改版（kinglf） | https://oshwhub.com/kinglf/pd-xie-yi-gao-yan-zhi-mini-jia-re-tai | 差异事实源 | 2023-12-26 13:22:01 | 2024-02-23 13:50:19 | GPL 3.0 | 不存在（页面显示暂无数据） | 存在（首屏可见） | 2026-03-03 |

依据：[S1][S2]

## 次级来源（仅交叉参考）

| 来源 | URL | 当前状态 | 用途 |
| --- | --- | --- | --- |
| 侵权投诉与申诉规则（S3） | https://lceda.cn/page/appeal | 存在 | 合规边界补充 |
| EEWorld 转载页（S4） | https://www.eeworld.com.cn/RDesigns_detail/79318 | 未检查 | 辅助确认附件命名 |

依据：[S3]

## 附件与下载可用性（三态）

### S1 原版

| 项目 | 页面可见性 | 备注 |
| --- | --- | --- |
| `mini加热台.zip` | 存在 | 页面展示下载次数 11881（采集时） |
| `演示.mp4` | 存在 | 页面展示下载次数 6362（采集时） |
| `HeatingPlate-PD-修复温度频繁跳变.hex` | 存在 | 页面展示下载次数 3354（采集时） |
| 附件内容校验和 | 未检查 | 未执行文件下载与哈希比对 |

依据：[S1]

### S2 铜柱二改

| 项目 | 页面可见性 | 备注 |
| --- | --- | --- |
| 附件表 | 不存在 | 页面显示“暂无数据” |
| 附件文件可下载性 | 未检查 | 无文件条目可验证 |

依据：[S2]

## 采集边界

- 登录后完整评论翻页：未检查。
- 购物链接商品参数与现价：未检查。
- 平台外转载内容（博客/论坛）：不作为主事实来源，除非后续逐条复核。

## 不可变证据（Immutable Evidence）

| 证据项 | 路径 | 状态 | 说明 |
| --- | --- | --- | --- |
| 采集清单与哈希 | `docs/research/mini-hotplate/evidence/source-manifest-2026-03-03.json` | 存在 | 记录 S1~S4 的抓取时间、HTTP 状态、内容 SHA256 |
| 证据说明 | `docs/research/mini-hotplate/evidence/README.md` | 存在 | 可人工复核 403/未检查来源 |

说明：S4 当前仅作为“待验证线索”，未纳入主结论依据。

## 引用

- [S1] https://oshwhub.com/littleoandlittleq/bian-xie-jia-re-tai
- [S2] https://oshwhub.com/kinglf/pd-xie-yi-gao-yan-zhi-mini-jia-re-tai
- [S3] https://lceda.cn/page/appeal
- [S4] https://www.eeworld.com.cn/RDesigns_detail/79318
