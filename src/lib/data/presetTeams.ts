/**
 * Preset teams — app-bundled, curated squads of agents you can deploy as a unit.
 *
 * These are starting points: a "team" is a hand-picked set pulled from across
 * divisions for a goal (ship a mobile app, run growth, etc.). Each references
 * corpus agent slugs (`<division>-<name>`); slugs not present in the active
 * catalog are silently skipped at deploy time, so a preset degrades gracefully.
 *
 * Curated by hand and bundled (no catalog/network dependency). To add one:
 * append an entry with a Lucide icon, a brand color, and verified slugs.
 */

import type { Component } from "svelte";
import Rocket from "@lucide/svelte/icons/rocket";
import Code from "@lucide/svelte/icons/code";
import TrendingUp from "@lucide/svelte/icons/trending-up";
import Compass from "@lucide/svelte/icons/compass";
import Sparkles from "@lucide/svelte/icons/sparkles";
import Shield from "@lucide/svelte/icons/shield";
import BarChart from "@lucide/svelte/icons/bar-chart";
import Headphones from "@lucide/svelte/icons/headphones";
import PenTool from "@lucide/svelte/icons/pen-tool";
import Gamepad2 from "@lucide/svelte/icons/gamepad-2";
import Scale from "@lucide/svelte/icons/scale";
import Activity from "@lucide/svelte/icons/activity";
import Accessibility from "@lucide/svelte/icons/accessibility";
import Globe from "@lucide/svelte/icons/globe";
import ShieldCheck from "@lucide/svelte/icons/shield-check";
import Brain from "@lucide/svelte/icons/brain";
import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
import Leaf from "@lucide/svelte/icons/leaf";
import Link from "@lucide/svelte/icons/link";
import Siren from "@lucide/svelte/icons/siren";
import Cpu from "@lucide/svelte/icons/cpu";
import Recycle from "@lucide/svelte/icons/recycle";
import Zap from "@lucide/svelte/icons/zap";
import FileText from "@lucide/svelte/icons/file-text";
import Users from "@lucide/svelte/icons/users";
import Handshake from "@lucide/svelte/icons/handshake";
import ShoppingCart from "@lucide/svelte/icons/shopping-cart";
import Briefcase from "@lucide/svelte/icons/briefcase";
import Gavel from "@lucide/svelte/icons/gavel";
import HeartPulse from "@lucide/svelte/icons/heart-pulse";
import Landmark from "@lucide/svelte/icons/landmark";
import Building from "@lucide/svelte/icons/building";
import ClipboardCheck from "@lucide/svelte/icons/clipboard-check";
import Eye from "@lucide/svelte/icons/eye";
import ShieldAlert from "@lucide/svelte/icons/shield-alert";
import Radar from "@lucide/svelte/icons/radar";
import Bug from "@lucide/svelte/icons/bug";
import FileSearch from "@lucide/svelte/icons/file-search";
import Sword from "@lucide/svelte/icons/sword";

export interface PresetTeam {
  slug: string;
  label: string;
  description: string;
  icon: Component;
  /** Brand color (hex) for the icon tint. */
  color: string;
  /** Corpus agent slugs that make up the team. */
  agents: string[];
}

export const PRESET_TEAMS: PresetTeam[] = [
  {
    slug: "mobile-launch",
    label: "Mobile Launch",
    description: "Take a mobile app from prototype to the App Store.",
    icon: Rocket,
    color: "#6366f1",
    agents: [
      "engineering-mobile-app-builder",
      "engineering-rapid-prototyper",
      "design-ui-designer",
      "design-ux-researcher",
      "marketing-app-store-optimizer",
    ],
  },
  {
    slug: "ship-web-app",
    label: "Ship It (Web)",
    description: "Build, deploy, and keep a web app healthy in production.",
    icon: Code,
    color: "#0ea5e9",
    agents: [
      "engineering-frontend-developer",
      "engineering-backend-architect",
      "engineering-devops-automator",
      "engineering-sre",
      "testing-api-tester",
    ],
  },
  {
    slug: "growth-squad",
    label: "Growth Squad",
    description: "Drive acquisition across content, search, and social.",
    icon: TrendingUp,
    color: "#ec4899",
    agents: [
      "marketing-growth-hacker",
      "marketing-content-creator",
      "marketing-seo-specialist",
      "marketing-tiktok-strategist",
      "marketing-instagram-curator",
    ],
  },
  {
    slug: "product-discovery",
    label: "Product Discovery",
    description: "Find what to build next and sequence the work.",
    icon: Compass,
    color: "#f59e0b",
    agents: [
      "product-sprint-prioritizer",
      "product-trend-researcher",
      "product-feedback-synthesizer",
      "design-ux-researcher",
    ],
  },
  {
    slug: "ai-builders",
    label: "AI Builders",
    description: "Ship AI features on a solid data + prompt foundation.",
    icon: Sparkles,
    color: "#8b5cf6",
    agents: [
      "engineering-ai-engineer",
      "engineering-prompt-engineer",
      "engineering-data-engineer",
      "engineering-backend-architect",
    ],
  },
  {
    slug: "security-hardening",
    label: "Security Hardening",
    description: "Audit, pentest, and harden systems before they ship.",
    icon: Shield,
    color: "#dc2626",
    agents: [
      "security-security-auditor",
      "security-penetration-tester",
      "security-incident-responder",
      "legal-compliance-checker",
    ],
  },
  {
    slug: "data-analytics",
    label: "Data & Analytics",
    description: "Turn raw data into models, dashboards, and decisions.",
    icon: BarChart,
    color: "#0ea5e9",
    agents: [
      "data-data-scientist",
      "data-analytics-engineer",
      "data-data-engineer",
      "engineering-ai-engineer",
    ],
  },
  {
    slug: "customer-support",
    label: "Customer Support & Success",
    description: "Delight users with fast, empathetic support and proactive success.",
    icon: Headphones,
    color: "#10b981",
    agents: [
      "support-support-agent",
      "support-customer-success-manager",
      "support-technical-support-engineer",
    ],
  },
  {
    slug: "content-social",
    label: "Content & Social",
    description: "Create and distribute content that grows an audience.",
    icon: PenTool,
    color: "#8b5cf6",
    agents: [
      "marketing-content-creator",
      "marketing-copywriter",
      "marketing-social-media-manager",
      "marketing-seo-specialist",
    ],
  },
  {
    slug: "game-dev",
    label: "Game Development",
    description: "Design, build, and polish a game from concept to launch.",
    icon: Gamepad2,
    color: "#f59e0b",
    agents: [
      "game-game-designer",
      "game-game-developer",
      "game-game-artist",
      "game-level-designer",
    ],
  },
  {
    slug: "finance-legal",
    label: "Finance & Legal",
    description: "Keep the books clean and the business compliant.",
    icon: Scale,
    color: "#6366f1",
    agents: [
      "finance-financial-analyst",
      "finance-accountant",
      "legal-legal-counsel",
      "legal-compliance-checker",
    ],
  },
  {
    slug: "devops-reliability",
    label: "DevOps & Reliability",
    description: "Operate production with confidence: SRE, incident response, and observability.",
    icon: Activity,
    color: "#0ea5e9",
    agents: [
      "engineering-sre",
      "engineering-incident-response-commander",
      "engineering-devops-automator",
      "engineering-observability-reviewer",
    ],
  },
  {
    slug: "accessibility-inclusive-design",
    label: "Accessibility & Inclusive Design",
    description: "Audit, remediate, and design for users with disabilities—WCAG, assistive tech, and inclusive patterns.",
    icon: Accessibility,
    color: "#0891b2",
    agents: [
      "testing-accessibility-auditor",
      "design-ux-researcher",
      "design-ui-designer",
      "engineering-frontend-developer",
    ],
  },
  {
    slug: "internationalization-localization",
    label: "Internationalization & Localization",
    description: "Make software correct across languages, scripts, and regions—not just translated.",
    icon: Globe,
    color: "#0d9488",
    agents: [
      "engineering-i18n-engineer",
      "design-ux-architect",
      "marketing-content-creator",
      "testing-reality-checker",
    ],
  },
  {
    slug: "data-privacy-compliance",
    label: "Data Privacy & Compliance",
    description: "Build GDPR/CCPA programs, privacy impact assessments, consent management, and breach response.",
    icon: ShieldCheck,
    color: "#7c3aed",
    agents: [
      "specialized-data-privacy-officer",
      "legal-compliance-checker",
      "engineering-security-engineer",
      "support-legal-compliance",
    ],
  },
  {
    slug: "ai-ethics-safety",
    label: "AI Ethics & Safety",
    description: "Review models and agents for bias, safety, transparency, and responsible AI deployment.",
    icon: Brain,
    color: "#db2777",
    agents: [
      "specialized-ai-ethics-officer",
      "engineering-ai-engineer",
      "product-behavioral-nudge-engine",
      "academic-psychologist",
    ],
  },
  {
    slug: "open-source-devex",
    label: "Open Source & Developer Experience",
    description: "Maintain healthy open‑source projects, grow contributor communities, and improve DX.",
    icon: GitPullRequest,
    color: "#f97316",
    agents: [
      "specialized-developer-advocate",
      "specialized-open-source-maintainer",
      "marketing-technical-writer",
      "engineering-codebase-onboarding-engineer",
    ],
  },
  {
    slug: "sustainability-esg",
    label: "Sustainability & ESG",
    description: "Measure, reduce, and report the environmental and social impact of software and operations.",
    icon: Leaf,
    color: "#16a34a",
    agents: [
      "specialized-esg-sustainability-officer",
      "engineering-sre",
      "finance-financial-analyst",
      "specialized-supply-chain-analyst",
    ],
  },
  {
    slug: "blockchain-smart-contracts",
    label: "Blockchain & Smart Contracts",
    description: "Design, audit, and operate decentralized systems and smart‑contract workflows.",
    icon: Link,
    color: "#ca8a04",
    agents: [
      "engineering-blockchain-developer",
      "specialized-smart-contract-auditor",
      "security-penetration-tester",
      "finance-investment-researcher",
    ],
  },
  {
    slug: "incident-response-crisis",
    label: "Incident Response & Crisis Management",
    description: "Coordinate technical incidents, communicate with stakeholders, and run post‑mortems.",
    icon: Siren,
    color: "#dc2626",
    agents: [
      "engineering-incident-response-commander",
      "support-customer-support",
      "marketing-crisis-communicator",
      "project-management-project-shepherd",
    ],
  },
  {
    slug: "edge-iot-embedded",
    label: "Edge, IoT & Embedded",
    description: "Build and secure firmware, edge runtimes, and connected‑device systems.",
    icon: Cpu,
    color: "#0f766e",
    agents: [
      "engineering-embedded-firmware-engineer",
      "engineering-edge-computing-engineer",
      "security-iot-security-specialist",
      "testing-performance-benchmarker",
    ],
  },
  {
    slug: "legacy-modernization",
    label: "Legacy Modernization",
    description: "Assess, refactor, and migrate legacy systems without breaking the business.",
    icon: Recycle,
    color: "#78716c",
    agents: [
      "engineering-codebase-onboarding-engineer",
      "engineering-backend-architect",
      "specialized-workflow-architect",
      "project-manager-senior",
    ],
  },
  {
    slug: "rapid-prototyping-hackathon",
    label: "Rapid Prototyping & Hackathon",
    description: "From idea to demo in hours—scrappy research, fast builds, and pitch‑ready storytelling.",
    icon: Zap,
    color: "#eab308",
    agents: [
      "specialized-rapid-prototyper",
      "engineering-frontend-developer",
      "design-whimsy-injector",
      "marketing-content-creator",
    ],
  },
  {
    slug: "technical-writing-docs",
    label: "Technical Writing & Documentation",
    description: "Produce API references, tutorials, runbooks, and developer‑facing docs that actually help.",
    icon: FileText,
    color: "#2563eb",
    agents: [
      "marketing-technical-writer",
      "specialized-developer-advocate",
      "engineering-codebase-onboarding-engineer",
      "design-ux-researcher",
    ],
  },
  {
    slug: "community-developer-education",
    label: "Community & Developer Education",
    description: "Grow and nurture developer communities, create learning content, and run evangelism programs.",
    icon: Users,
    color: "#9333ea",
    agents: [
      "marketing-community-manager",
      "specialized-developer-advocate",
      "marketing-content-creator",
      "support-customer-success-manager",
    ],
  },
  {
    slug: "partner-channel-management",
    label: "Partner & Channel Management",
    description: "Build partner programs, enable resellers, and manage co‑selling relationships.",
    icon: Handshake,
    color: "#0ea5e9",
    agents: [
      "sales-partner-manager",
      "sales-account-strategist",
      "marketing-partner-marketing-manager",
      "support-customer-success-manager",
    ],
  },
  {
    slug: "procurement-vendor-management",
    label: "Procurement & Vendor Management",
    description: "Source, evaluate, and manage vendors while controlling spend and risk.",
    icon: ShoppingCart,
    color: "#64748b",
    agents: [
      "finance-procurement-specialist",
      "legal-compliance-checker",
      "finance-financial-analyst",
      "specialized-supply-chain-analyst",
    ],
  },
  {
    slug: "ma-due-diligence",
    label: "M&A & Due Diligence",
    description: "Evaluate targets, assess technical and financial risk, and support integration planning.",
    icon: Briefcase,
    color: "#1e293b",
    agents: [
      "finance-ma-analyst",
      "legal-compliance-checker",
      "engineering-codebase-onboarding-engineer",
      "project-manager-senior",
    ],
  },
  {
    slug: "regulatory-legal-compliance",
    label: "Regulatory & Legal Compliance",
    description: "Navigate industry regulations, audits, and cross‑jurisdictional legal requirements.",
    icon: Gavel,
    color: "#7f1d1d",
    agents: [
      "legal-legal-counsel",
      "legal-compliance-checker",
      "specialized-regulatory-compliance-specialist",
      "support-legal-compliance",
    ],
  },
  {
    slug: "healthcare-life-sciences",
    label: "Healthcare & Life Sciences",
    description: "Build compliant health software, handle PHI, and navigate clinical workflows.",
    icon: HeartPulse,
    color: "#be123c",
    agents: [
      "specialized-healthcare-compliance-officer",
      "specialized-clinical-workflow-specialist",
      "engineering-security-engineer",
      "legal-compliance-checker",
    ],
  },
  {
    slug: "fintech-financial-compliance",
    label: "Fintech & Financial Compliance",
    description: "Ship financial products while meeting KYC/AML, PCI, and regulatory obligations.",
    icon: Landmark,
    color: "#0369a1",
    agents: [
      "finance-fintech-compliance-specialist",
      "security-pci-auditor",
      "engineering-payments-engineer",
      "legal-compliance-checker",
    ],
  },
  {
    slug: "government-public-sector",
    label: "Government & Public Sector",
    description: "Deliver civic technology with accessibility, procurement, and sovereignty requirements.",
    icon: Building,
    color: "#334155",
    agents: [
      "specialized-govtech-specialist",
      "testing-accessibility-auditor",
      "legal-compliance-checker",
      "project-manager-senior",
    ],
  },
  {
    slug: "full-review-board",
    label: "Full Review Board",
    description: "Complete pre-ship review—readability, reliability, resilience, observability, risk, security, and an adversarial refuter.",
    icon: ClipboardCheck,
    color: "#0f172a",
    agents: [
      "engineering-review-readability",
      "engineering-review-reliability",
      "engineering-review-resilience",
      "engineering-review-observability",
      "engineering-review-risk",
      "engineering-review-security",
      "specialized-review-refuter",
    ],
  },
  {
    slug: "code-quality-review",
    label: "Code Quality Review",
    description: "Readability, reliability, and resilience reviews with a refuter to challenge assumptions before merge.",
    icon: Eye,
    color: "#2563eb",
    agents: [
      "engineering-review-readability",
      "engineering-review-reliability",
      "engineering-review-resilience",
      "specialized-review-refuter",
    ],
  },
  {
    slug: "security-risk-review",
    label: "Security & Risk Review",
    description: "Security, risk, and adversarial review—catches threats, unsafe assumptions, and blast-radius issues.",
    icon: ShieldAlert,
    color: "#dc2626",
    agents: [
      "engineering-review-security",
      "engineering-review-risk",
      "specialized-review-refuter",
      "engineering-review-reliability",
    ],
  },
  {
    slug: "operational-readiness-review",
    label: "Operational Readiness Review",
    description: "Confirm a service is observable, reliable, and resilient before it goes to production.",
    icon: Radar,
    color: "#0891b2",
    agents: [
      "engineering-review-observability",
      "engineering-review-reliability",
      "engineering-review-resilience",
    ],
  },
  {
    slug: "adversarial-review",
    label: "Adversarial Review",
    description: "Red-team style review—refuter leads, supported by risk, security, and readability reviewers who pressure-test every claim.",
    icon: Sword,
    color: "#7c3aed",
    agents: [
      "specialized-review-refuter",
      "engineering-review-risk",
      "engineering-review-security",
      "engineering-review-readability",
    ],
  },
  {
    slug: "bug-hunt-review",
    label: "Bug Hunt Review",
    description: "Focused hunt for defects—reliability and resilience reviewers dig for failure modes while the refuter challenges fixes.",
    icon: Bug,
    color: "#f59e0b",
    agents: [
      "engineering-review-reliability",
      "engineering-review-resilience",
      "specialized-review-refuter",
      "engineering-review-observability",
    ],
  },
  {
    slug: "minimal-review-pair",
    label: "Minimal Review Pair",
    description: "Lightweight two-agent gate—readability plus a refuter—for small PRs and hotfixes.",
    icon: FileSearch,
    color: "#64748b",
    agents: [
      "engineering-review-readability",
      "specialized-review-refuter",
    ],
  },
];
