# Runbook: Accessibility Remediation Sprint
>
> **Mode**: NEXUS-Micro | **Duration**: 1-2 weeks | **Agents**: 8-10

## Scenario

A compliance deadline or accessibility complaint has created urgency. The product has known or suspected failures and no current audit. The remediation sprint audits, prioritizes, and fixes accessibility failures against WCAG 2.2 AA or Section 508 — with evidence, not good intentions.

## Severity Classification

| Level | WCAG Mapping | Impact | Remediation SLA |
| --- | --- | --- | --- |
| **Critical** | Level A, blocks task | User cannot complete the task | Fix within 48 hours |
| **Serious** | Level AA, causes error/confusion | User completes with significant difficulty | Fix within sprint |
| **Moderate** | Level AA, inconvenient | Extra effort but recoverable | Fix within sprint or schedule |
| **Minor** | Level AAA or cosmetic | No functional impact | Backlog with target |

## Response Teams by Severity

### Remediation Core

| Agent | Role |
| --- | --- |
| Agents Orchestrator | Sprint facilitation, fix sequencing |
| Senior Project Manager | Scope control, deliverable tracking |
| Section 508 Specialist | Standard interpretation and conformance mapping |
| Accessibility Auditor | Audit and assistive-technology testing |
| UI Designer | Design-level fixes (contrast, focus, target size) |
| Frontend Developer | Implementation fixes |
| Evidence Collector | Before/after evidence per finding |

### Advisory

| Agent | Role |
| --- | --- |
| Legal Compliance Checker | Deadline obligations, disclosure language |
| Inclusive Visuals Specialist | Alt-text strategy, non-text content |

## Remediation Sequence

### Step 1: Audit (Days 1-3)

```text
Section 508 Specialist:
├── Confirm target standard and conformance level (WCAG 2.2 AA vs. Section 508 — not interchangeable)
└── Pin one standard before auditing

Accessibility Auditor:
├── Automated tooling across all templates and states
├── Manual keyboard testing of every interactive flow
├── Screen reader testing (at least two ATs)
├── 200% zoom, high-contrast mode, reduced motion
└── Record each finding: location, criterion, severity, reproduction steps, evidence

Output: Finding list with severity and reproduction steps
```

### Step 2: Prioritize (Day 3)

```text
├── Rank by Critical → Serious → Moderate → Minor
├── Legal Compliance Checker flags direct compliance exposure
├── Senior Project Manager confirms what fits the window
├── Frontend Developer estimates each fix; anything over one day is scoped down
├── Deferred items documented with a scheduled follow-on
└── Quality Gate: Prioritized fix list with owners and estimates
```

### Step 3: Remediate (Days 4-7)

```text
Fix order:
1. Structural first — focus order, landmarks, form labeling, keyboard traps
2. UI Designer → Contrast, target size, focus visibility at design level
3. Frontend Developer → Implementation
4. Accessibility Auditor → Re-test with the same AT that found the issue
5. Evidence Collector → Before/after per finding

Rules:
├── Cosmetic fixes last — structural fixes often change them
├── Any fix that introduces a regression is reverted, not patched over
└── Every fix has before/after evidence

Output: Critical and serious findings closed with evidence
```

### Step 4: Verify & Report (Days 8-10)

```text
Accessibility Auditor:
├── Full re-audit, not just fixed items
└── Confirm no new failures introduced

Section 508 Specialist:
├── Write the conformance statement: standard, level, what conforms, what does not, plan for exceptions

Legal Compliance Checker:
└── Review statement language

Evidence Collector:
└── Assemble final evidence pack indexed by criterion

Output: Clean pass on critical/serious, conformance statement, evidence pack
```

## Communication Templates

### Internal Status Update (Agents Orchestrator)

```text
[Timestamp] — Accessibility Remediation Update

SCOPE: [Standard, level]
FINDINGS: [Count by severity]
CRITICAL CLOSED: [n / total]
SERIOUS CLOSED: [n / total]
DEFERRED: [Count with scheduled dates]
NEXT MILESTONE: [Date and deliverable]
```

### Conformance Statement (Section 508 Specialist)

```text
Accessibility Conformance Statement — [Date]

STANDARD: [WCAG 2.2 AA / Section 508 / both]
SCOPE: [Templates and flows audited]
CONFORMANCE: [Conforms / Partially conforms]
KNOWN EXCEPTIONS: [List with remediation dates]
EVIDENCE: [Reference to evidence pack]
CONTACT: [Owner]
```

## Escalation Matrix

| Condition | Escalate To | Action |
| --- | --- | --- |
| Deadline is imminent and critical findings remain open | Legal Compliance Checker | Scope reduction and disclosure plan |
| Fix introduces regression | Reality Checker | Revert and re-approach |
| Standard ambiguity blocks decisions | Section 508 Specialist | Interpretation and written rationale |
| Deferred items exceed the window | Senior Project Manager | Schedule follow-on with date and owner |
