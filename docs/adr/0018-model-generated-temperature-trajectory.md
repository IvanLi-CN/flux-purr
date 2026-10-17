# Model-generated temperature reference for Approach

## Status

Accepted by the Operator. Refines [ADR-0017](0017-unified-production-thermal-controller.md).

## Context

Approach previously derived a desired rise from instantaneous final-target error and held minimum power for the calibrated transport delay. It had no independently evolving temperature reference. A high-loss plate can cool before reactive recovery; changing legacy power bounds does not establish trajectory tracking.

## Decision

Warmup remains independent full-power heating. Approach starts with an immediate 5% braking request, then tracks an explicit model-generated temperature/rate reference. Calibrated losses, capacity and source capability bound feasible reference rates. Braking distance controls handoff; damping and lead shape the reference response. Legacy floor/ceiling fields do not constrain the model controller.

The tracker predicts command-arrival temperature using bounded, time-weighted applied-power history. Model feedforward and temperature/rate feedback determine subsequent 5%–100% requests. Applied power is estimated from VIN, physical duty and resistance, never presented as measured electrical power. Hold tracks the constant final target using the protected PI. PD/PPS ownership, voltage/PWM mapping and source limits remain unchanged.

After the initial braking request, accepted measurements permit the full 5%–100% model response without a separate actuator slew limit; downward braking remains immediate. Rejected observations cannot raise the last applied request. Stable-window qualification consumes only accepted measurements. A bounded PPS sampling pause identified by a request change or its matching confirmed contract change may retain a provisional start within the existing one-second sampling-gap bound; rejected values never count as witnesses or complete qualification. Other rejected observations reset continuity. The timing policy and raw evidence remain explicit in reports.

## Consequences

有效模型Approach的欠温恢复保留阶段参考并由现有5%–100%跟踪补热执行。旧profile的温差重入Warmup只适用于非模型兼容路径，不能清掉模型轨迹或重启软启动；固定Hold入口、首次Warmup退出期限及真正停热/故障生命周期保持独立。

共享预测器使用受约束的余热差额观测修正纯延迟模型没有表达的传热。观测以本轮真实输入和有效档内温差的能量平衡为依据，不将完整散热补偿再次加到历史供热。只在已知历史上使用限幅、有限寿命的差额；PPS换档先重建原点，未知历史、拒绝测温与过期状态不构成观测。参考生成、Hold受保护PI、PD/PPS以及资格门禁保持各自职责，具体时间窗口与公式由热控SPEC关联的自然冷却控制合同定义。

PPS measurement epochs neither restart the reference nor erase heat in flight. Actual temperature remains authoritative for phase entry and qualification; reaching a reference cannot qualify a trial. Initial points use fresh calibration and production constants; later search uses only the same run. Both source classes require exact-artifact eight-target HIL and independent confirmations before saving.

## Alternatives considered

A fixed minimum-power wait ignores predictable loss. Instantaneous final-target feedback cannot represent the approved independent decelerating response. Freezing PPS voltage or relaxing Hold/time gates changes separate contracts without repairing this control boundary.
