# Runbook: Data Platform Foundation

Mode: NEXUS-Sprint | Duration: 8-12 weeks | Agents: 14-16

## Scenario

You are building a reliable, governed data platform — ingestion, lakehouse, transformation, and analytics-ready assets — without sacrificing observability or compliance. The organization has data spread across systems, analysts are querying production replicas, and the output is a platform other teams build on, not a one-off pipeline.

## Agent Roster

### Core Platform Team

| Agent | Role |
| --- | --- |
| Agents Orchestrator | Pipeline controller |
| Senior Project Manager | Spec-to-task conversion |
| Product Manager | Priority questions and success metrics |
| Data Engineer | Ingestion, lakehouse layout, transformation logic |
| Database Optimizer | Source extraction performance, warehouse tuning |
| Backend Architect | Service boundaries, CDC strategy, event contracts |
| DevOps Automator | IaC, pipeline orchestration, environment parity |
| SRE | Freshness SLAs, alerting, on-call runbooks |
| API Tester | Source connector contracts, schema-drift tests |
| Evidence Collector | Data quality evidence, lineage traceability |

### Analytics & Remediation

| Agent | Role |
| --- | --- |
| Analytics Reporter | First consumer; validates the platform answers real questions |
| Data Visualization Engineer | Semantic layer and BI consumption patterns |
| AI Data Remediation Engineer | Anomaly detection and automated quality remediation |

### Governance & Stakeholders

| Agent | Role |
| --- | --- |
| Legal Compliance Checker | PII classification, retention, residency, DSR handling |
| Executive Summary Generator | Steering updates and funding-case narrative |

## Execution Plan

### Phase 1: Discovery & Data Contracts (Weeks 1-2)

```text
Week 1: Source Inventory
├── Product Manager → Priority questions ranked
├── Data Engineer → Source inventory: volume, cadence, change pattern, owner
├── Backend Architect → CDC/extraction strategy per source
├── Legal Compliance Checker → Field classification (public/internal/confidential/PII/regulated)
└── Quality Gate: Source inventory complete with named owners

Week 2: Contracts
├── Database Optimizer → Source extraction cost profile
├── Backend Architect → Event contracts and downstream dependencies
├── Legal Compliance Checker → Retention and residency decisions recorded
├── Project Shepherd → Load-window constraints documented
└── Quality Gate: Extraction strategy agreed per source
```

### Phase 2: Ingestion & Lakehouse (Weeks 2-6)

```text
Sprint 1:
├── DevOps Automator → IaC for dev/staging/prod
├── Data Engineer → Top three sources ingestion
├── API Tester → Schema-drift tests per connector
├── SRE → Freshness SLAs and alerting
├── Backend Architect → Raw/bronze zone layout
└── Quality Gate: Top three sources ingest on schedule

Sprint 2:
├── Agents Orchestrator → Dev↔QA loop
├── Data Engineer → Remaining sources, reprocessing strategy
├── API Tester → Drift tests fail correctly on contract violation
├── Evidence Collector → QA on every connector
└── Quality Gate: Historical window reprocessing works without manual repair
```

### Phase 3: Transformation & Semantic Layer (Weeks 5-8)

```text
├── Data Engineer → Bronze → silver → gold transformations with tests
├── Data Visualization Engineer → Semantic layer: metrics, dimensions, term definitions
├── Analytics Reporter → Consume for the three priority questions
├── Database Optimizer → Heavy-model tuning and cost curve
├── Evidence Collector → Lineage evidence for every gold table
└── Quality Gate: Three priority questions answerable without bespoke queries
```

### Phase 4: Governance & Observability (Weeks 8-10)

```text
├── Legal Compliance Checker → PII handling, retention jobs, DSR drill
├── SRE → Four on-call runbooks (freshness, volume, drift, upstream outage)
├── AI Data Remediation Engineer → Anomaly detection on volume and distribution
├── DevOps Automator → Model promotion path documented
├── Evidence Collector → DSR drill evidence
└── Quality Gate: DSR drill completed end-to-end
```

### Phase 5: Handover & Adoption (Weeks 10-12)

```text
├── Analytics Reporter → Onboard first two consuming teams
├── Executive Summary Generator → Funding/ROI narrative with measured numbers
├── Senior Project Manager → Explicit non-goals documented
├── SRE → Ownership transferred with named on-call
└── Quality Gate: Two consuming teams shipped without platform-team intervention
```

## Key Decisions

| Decision Point | When | Who Decides |
| --- | --- | --- |
| Data classification | Week 1 | Legal Compliance Checker |
| CDC vs. batch per source | Week 2 | Backend Architect |
| First three sources | Sprint 1 planning | Product Manager + Data Engineer |
| Business metric definitions | Phase 3 | Data Visualization Engineer + Analytics Reporter |
| Platform go-live | Week 10 | Evidence Collector + SRE |

## Success Criteria

| Metric | Target |
| --- | --- |
| Priority questions answerable | 3 of 3 |
| Source freshness SLAs met | Per source target |
| One definition per business metric | 100% |
| Lineage visible on gold tables | 100% |
| DSR drill | Completed with evidence |
| Consuming teams onboarded | ≥ 2 without platform-team intervention |

## Common Pitfalls & Mitigations

| Pitfall | Mitigation |
| --- | --- |
| Platform answers one question | Product Manager ranks and gates on top three |
| Schema drift corrupts tables | API Tester writes drift tests per connector |
| Two definitions of "active user" | Semantic layer owns definitions in Phase 3 |
| Compliance addressed at the end | Legal Compliance Checker active from Week 1 |
| Platform team is the only user | Analytics Reporter onboards consumers in Phase 5 |
