# EdgeOne 发布失败分类

## Status

Accepted

## Context

EdgeOne CLI 1.6.18 在两个 API endpoint 都无法完成请求时，会把 endpoint、认证和项目访问问题压缩成 `Invalid EDGEONE_PAGES_API_TOKEN`。该文案不足以证明凭据失效，尤其不能解释 522、5xx、timeout、connection reset 或 DNS 故障。发布 helper 需要保留 CLI 原始失败状态，同时给出可审查的失败类别。

## Decision

发布 helper 使用以下稳定失败类别：

- `credential_rejected`：远端明确返回 401、403 或明确的认证错误码。
- `project_or_permission`：远端明确指出项目不存在、项目访问被拒绝或项目权限不足。
- `endpoint_unavailable`：部署开始前识别出 5xx、522、timeout、connection reset、connection refused 或 DNS 类 transport failure。
- `deployment_ambiguous`：上传、创建 deployment、远端接受请求或状态查询开始后失败，远端状态无法安全确定。
- `auth_or_endpoint_ambiguous`：CLI 没有提供足够证据区分认证拒绝和 endpoint 故障。

每个失败终止路径在 stderr 输出 `EDGEONE_FAILURE_CLASS=<classification>`。模糊分类必须明确说明 Token 失效尚未得到证明，并保持失败退出码。

只有部署开始前、且输出满足既有 transport preflight 证据规则时允许有限重试。`deployment_ambiguous` 和 `auth_or_endpoint_ambiguous` 禁止自动重试；helper 不发起第二次 API 请求，也不改变 secret 注入、CLI 版本、发布 workflow 或 EdgeOne 调用参数。

## Consequences

GitHub Actions 日志可以区分凭据拒绝、项目权限、endpoint 不可用、远端状态不确定和 CLI 泛化错误。泛化错误仍然阻止发布，因此无法证明 Token 失效不会被误处理为成功。CLI 原始退出码和部署后的安全边界保持不变。

该分类依赖 CLI 输出证据。CLI 没有暴露 HTTP status、request ID 或认证错误码时，helper 只能报告 `auth_or_endpoint_ambiguous`，不能推断 Token 的有效期、scope 或账号归属。

## Alternatives considered

- 将所有 `Invalid EDGEONE_PAGES_API_TOKEN` 直接归类为凭据拒绝会继续把 endpoint 故障误报为 Token 失效。
- 通过第二次 API 请求探测 endpoint 会改变发布行为，并可能在远端状态不确定时扩大副作用，因此不采用。
- 在部署已经开始后自动重试可能产生重复 deployment，违反发布安全边界，因此不采用。
