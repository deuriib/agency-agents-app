# Runbook: AI Feature Integration

Mode: NEXUS-Sprint | Duration: 4-8 weeks | Agents: 14-16

## Scenario

You are shipping an LLM-powered feature into an existing product with prompt governance, evaluation, fallback behavior, and cost controls — not just a demo. The feature's output is user-visible, and the difference from a prototype is a measured quality bar and a degraded path when the model fails.

## Agent Roster

### Core AI Team

| Agent | Role |
| --- | --- |
| Agents Orchestrator | Pipeline controller, eval-gate enforcement |
| Senior Project Manager | Spec-to-task conversion |
| Product Manager | Quality bar, user-facing behavior, success metrics |
| AI Engineer | Model integration, retrieval, tooling, latency |
| Prompt Engineer | Prompt design, versioning, evaluation harness |
| Backend Architect | Service boundary, caching, rate limiting, cost caps |
| Frontend Developer | Streaming UX, loading/error states, feedback capture |
| Evidence Collector | Evaluation evidence, prompt-version traceability |
| Reality Checker | Adversarial and out-of-distribution testing |

### Safety & Evaluation

| Agent | Role |
| --- | --- |
| API Tester | Contract tests, malformed input, timeout behavior |
| Legal Compliance Checker | Data handling, PII in prompts, vendor terms |
| Multi-Agent Systems Architect | Orchestration design if tool/agent chaining |
| Risk Reviewer | Threat model: injection, exfiltration, misuse |

### Operations

| Agent | Role |
| --- | --- |
| DevOps Automator | Deploy pipeline, prompt/config rollout |
| Infrastructure Maintainer | Capacity, quotas, provider failover |
| Analytics Reporter | Quality, cost, usage dashboards |

## Execution Plan

### Phase 1: Scoping & Evaluation Design (Weeks 1-2)

```text
Week 1: Quality Bar
├── Product Manager → Numeric quality bar, latency ceiling, failure behavior
├── Prompt Engineer and AI Engineer → Evaluation set (≥ 100 labeled examples)
├── Backend Architect → Service boundary, caching, cost caps
├── Risk Reviewer → Threat model (injection, exfiltration, misuse)
└── Quality Gate: Quality bar documented with numeric thresholds

Week 2: Review
├── Legal Compliance Checker → Data-handling and vendor-terms review
├── Multi-Agent Systems Architect → Orchestration scope, if in use
├── Prompt Engineer → Evaluation set version-controlled
└── Quality Gate: Evaluation set built and threat model reviewed
```

### Phase 2: Prototype & Prompt Engineering (Weeks 2-4)

```text
Sprint 1:
├── AI Engineer → Thinnest working path (one model, one prompt, no retrieval)
├── Prompt Engineer → Iterate against evaluation set; commit every prompt version
├── Evidence Collector → Evaluation results per prompt version
├── Risk Reviewer → Adversarial cases reviewed
└── Quality Gate: Baseline measured

Sprint 2:
├── AI Engineer → Retrieval added only if needed, full eval re-run
├── Prompt Engineer → Regression delta per prompt version
├── Reality Checker → Out-of-distribution and adversarial cases
├── Evidence Collector → Prompt versions and scores traceable
└── Quality Gate: Scores meet quality bar reproducibly across 3 runs
```

### Phase 3: Integration (Weeks 3-5)

```text
├── Backend Architect → Timeouts, retries with backoff, circuit breaker, cost caps
├── Frontend Developer → Streaming, loading, and error states
├── API Tester → Malformed input, upstream timeout, provider 5xx, rate-limit responses
├── Infrastructure Maintainer → Quotas and provider failover
├── DevOps Automator → Prompt/config deployable independent of code, with rollback
├── Evidence Collector → QA on every increment
└── Quality Gate: Cost caps enforced by deliberate over-limit test
```

### Phase 4: Evaluation & Safety Gate (Weeks 5-6)

```text
├── Evidence Collector → Full eval against integrated feature, not the harness
├── Risk Reviewer → Threat model re-run against integrated system
├── Legal Compliance Checker → Data flow matches Phase 1 review
├── Reality Checker → Blind evaluation (outputs without prompt version)
├── Agents Orchestrator → Gate review
└── Quality Gate: No blocking security or compliance findings; quality bar met
```

### Phase 5: Rollout & Monitoring (Weeks 6-8)

```text
├── DevOps Automator → Staged rollout behind flag (internal → 5% → 25% → 100%)
├── Analytics Reporter → Usage, quality signal, latency, cost, fallback rate dashboard
├── Frontend Developer → Lightweight feedback affordance into the eval set
├── Support Responder → Failure-pattern watching
├── Agents Orchestrator → Week 8 retro
└── Quality Gate: Quality and cost within bounds at 100%
```

## Key Decisions

| Decision Point | When | Who Decides |
| --- | --- | --- |
| Quality bar and latency ceiling | Week 1 | Product Manager + Risk Reviewer |
| Retrieval required | Phase 2 | AI Engineer + Evidence Collector |
| Cost caps | Week 1 | Backend Architect |
| Safety gate pass | Week 6 | Risk Reviewer + Legal Compliance Checker |
| Rollout pace | Week 6 | DevOps Automator |

## Success Criteria

| Metric | Target |
| --- | --- |
| Quality bar met reproducibly | ≥ 3 runs |
| Prompt versions traceable | 100% |
| Degraded path tested | Yes (timeout, rate limit, provider 5xx) |
| Cost caps enforced | Verified by over-limit test |
| Blocking security/compliance findings | Zero at gate |
| Blind evaluation | Completed |

## Common Pitfalls & Mitigations

| Pitfall | Mitigation |
| --- | --- |
| "Works in a demo" | Prompt Engineer builds eval set before tuning |
| Prompt versions drift | Every prompt committed and scored |
| No failure experience | Frontend Developer specifies degraded UX |
| Injection via user content | Risk Reviewer includes injection in threat model |
| Cost overruns after launch | Backend Architect enforces caps from Phase 1 |
