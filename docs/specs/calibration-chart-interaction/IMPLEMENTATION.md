# 校准图表数据悬浮交互实现

## Implementation coverage

- `TemperatureRequestChart` 和温度轨迹图共享指针、触摸、长按和左右方向键交互，并在有效 X 轴范围内显示线性插值结果。
- 插值结果使用参考线和独立高亮点呈现；有效范围外显示无有效拟合/轨迹数据，不进行外推。
- 温度—申请电压图的离散点、供电观测表按钮和选中行保持同步，选中行只在表体滚动容器内调整可见位置。
- 供电观测表标题、来源和表头固定，表体使用固定高度的 SimpleBar 独立滚动区域，移动端保留横向查看能力。
- 已移除两张图表底部重复的 `thermal-chart__detail` 选中摘要。

## Validation

- `bun run check` 通过。
- `bun run typecheck` 通过。
- `bun run build` 通过。
- 相关 Vitest 通过：2 个文件、28 个测试。
- 全量 Storybook 测试通过：9 个 story 文件、152 个测试。
- 浏览器实测覆盖桌面/窄屏悬浮窗、左右边界定位、选中行滚动、表头固定和轨迹图键盘交互。

## Remaining gaps

- 当前视觉证据为新规格下的 current-only 截图，尚无已确认的基线截图。
