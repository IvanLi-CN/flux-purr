# 正式控制台与演示运行时分离

## Status

Accepted；Web 实现已按本决策收敛，发布仍由现有双构建 EdgeOne 流程负责。

## Context

Flux Purr 的正式版与 Demo 已分别部署到 EdgeOne Makers，公开 Demo 入口为 <https://flux-purr-demo.ivanli.cc/>。公开 Demo 使用独立静态构建并强制使用 mock fixture；正式 Web 构建固定服务真实运行时。两种运行时复用正式产品组件和设备路由，但入口含义由构建确定。

## Decision

正式站点固定为 live，演示只由独立 Demo 构建与站点承载。两者继续复用正式产品页面、路由及交互契约，由构建对应的运行时确定真实或模拟设备能力。URL 和本地偏好不得改变该运行时边界；Demo 的初始化失败必须停留在安全错误状态，不得回退到真实设备或真实服务。

分享链接复现演示场景、当前设备、故障及能力/数据初始条件。产品操作产生的临时模拟结果与模拟操作记录不属于分享场景，链接不承担操作回放职责。演示运行时可以维护临时内存状态，设备凭据不进入分享内容。

## Consequences

整改沿用已有两套 EdgeOne Makers 部署。正式构建收到旧 `demo=true` 或 `uiDemo` 入口时显示停用页，不回退到 Demo，也不把旧演示链接静默变成真实设备控制；Demo 构建清理这些旧模式参数并保持 mock。页面级验收分别证明正式构建的运行时固定性与 Demo 构建的真实 I/O 隔离。

## Alternatives considered

保留正式站点内的 query / 本地偏好模式切换可以继续复用既有入口，但会让同一正式 URL 的设备能力依赖访问历史，且需要长期维护站内运行时切换的状态隔离。独立构建与站点保留共享产品界面，同时使入口含义与设备边界保持稳定。

## References

- [产品领域用语](../../GLOSSARY.md)
- [Web Demo 规格](../specs/web-control-plane-demo/SPEC.md)
- [应用模式选择](../../web/src/app-mode.ts)
- [公开 Demo 构建标识](../../web/src/public-demo.ts)
