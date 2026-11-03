# Runbook: E-commerce Storefront Launch

Mode: NEXUS-Sprint | Duration: 4-6 weeks | Agents: 16-18

## Scenario

You are launching a conversion-focused storefront with payments, catalog, and content wired correctly. QA and brand consistency are enforced before the first real order. Revenue systems fail loudly and expensively; this runbook front-loads the parts that cannot be fixed after launch.

## Agent Roster

### Core Commerce Team

| Agent | Role |
| --- | --- |
| Agents Orchestrator | Pipeline controller |
| Senior Project Manager | Spec-to-task conversion |
| Product Manager | Conversion definition and launch criteria |
| UI Designer | PDP, PLP, cart, checkout design |
| UX Researcher | Purchase-flow usability |
| Frontend Developer | Storefront implementation and performance |
| Backend Architect | Catalog, cart, order, inventory services |
| CMS Developer | Content model and merchandising workflow |
| Payments & Billing Engineer | Payment integration, tax, refunds, idempotency |
| Evidence Collector | Checkout evidence per step |
| API Tester | Cart, order, inventory contract tests |
| Reality Checker | Real-device, real-network purchase testing |

### Growth & Content

| Agent | Role |
| --- | --- |
| Growth Hacker | Funnel instrumentation and experiments |
| Content Creator | Product copy, landing pages, lifecycle emails |
| Social Media Strategist | Launch amplification plan |

### Operations & Support

| Agent | Role |
| --- | --- |
| Support Responder | Order inquiries, returns, launch-week triage |
| Analytics Reporter | Conversion, AOV, abandonment reporting |
| Legal Compliance Checker | Consumer law, tax, privacy, cookie consent |

## Execution Plan

### Phase 1: Commerce Model & Design (Weeks 1-2)

```text
Week 1: Model
├── Product Manager → Conversion definition and launch threshold
├── Backend Architect → Catalog model, cart/order lifecycle with edge cases
├── Payments & Billing Engineer → Payment, tax, refund flow, idempotency design
├── UX Researcher → Paper-prototype purchase walkthrough (5 users)
└── Quality Gate: Catalog and cart/order models documented with edge cases

Week 2: Design
├── UI Designer → PDP, PLP, cart, and checkout screens
├── CMS Developer → Content model and merchandising ownership
├── Legal Compliance Checker → Terms, privacy, cookie consent, tax display
├── Brand Guardian → Design consistency review
└── Quality Gate: Usability walkthrough completed and findings triaged
```

### Phase 2: Storefront Build (Weeks 2-4)

```text
Sprint 1:
├── Frontend Developer → Catalog and content surfaces
├── Payments & Billing Engineer → Checkout against sandbox (declines, 3-D Secure, refunds)
├── API Tester → Cart, order, inventory contract tests
├── Backend Architect → Inventory decrement and oversell protection
└── Quality Gate: Full purchase completes in staging with sandbox payment

Sprint 2:
├── Agents Orchestrator → Dev↔QA loop
├── API Tester → Concurrent-purchase race tests
├── Evidence Collector → QA on checkout flow
├── Reality Checker → Decline, refund, and edge-case verification
└── Quality Gate: Oversell protection verified under concurrent load
```

### Phase 3: Hardening & Content (Weeks 4-5)

```text
├── Frontend Developer → Core Web Vitals budgets on PDP and checkout (mobile, throttled)
├── Evidence Collector → Full purchase regression across desktop, mobile, tablet
├── Reality Checker → Real-device real-card purchase, unassisted
├── Content Creator and CMS Developer → Real catalog and merchandising content
├── Growth Hacker → Funnel instrumentation (PLP, PDP, ATC, checkout, purchase)
├── Brand Guardian → Storefront, emails, launch assets
└── Quality Gate: Zero placeholder content in production paths
```

### Phase 4: Launch & Stabilize (Weeks 5-6)

```text
├── DevOps Automator → Soft launch to limited segment
├── Support Responder → Order-inquiry queue from hour one
├── Analytics Reporter → Daily conversion and abandonment reporting
├── Growth Hacker → First pricing/merchandising experiment after baseline
├── Legal Compliance Checker → Post-launch compliance confirmation
└── Quality Gate: Order success rate at target for 72 consecutive hours
```

## Key Decisions

| Decision Point | When | Who Decides |
| --- | --- | --- |
| Conversion definition and launch threshold | Week 1 | Product Manager |
| Payment, tax, refund design | Week 1 | Payments & Billing Engineer |
| Content model ownership | Week 2 | CMS Developer |
| Launch readiness | Week 5 | Reality Checker + Evidence Collector |
| Rollout pace | Week 5 | DevOps Automator |

## Success Criteria

| Metric | Target |
| --- | --- |
| Purchase completes end-to-end in staging | Yes, including decline and refund |
| Oversell protection verified | Under concurrent load |
| Core Web Vitals on PDP and checkout | Met |
| Funnel instrumentation | Verified at every step |
| Placeholder content in production paths | Zero |
| Order success rate for 72 hours | At or above target |

## Common Pitfalls & Mitigations

| Pitfall | Mitigation |
| --- | --- |
| Duplicate orders at launch | Payments & Billing Engineer designs idempotency in Week 1 |
| Oversell under concurrent load | Backend Architect implements and API Tester verifies |
| Placeholder content ships | Content Creator and CMS Developer load real catalog before launch |
| Checkout friction found late | UX Researcher paper prototype in Week 1 |
| Funnel blind at launch | Growth Hacker instruments before soft launch |
