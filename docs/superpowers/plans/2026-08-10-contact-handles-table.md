# Contact Handles Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Contact drawer handles table with per-handle stats, inline edit, unlink, and conversation browse links.

**Architecture:** Extend contact detail GET stats; implement POST mutations; rebuild drawer UI as a table.

**Tech Stack:** Rust contacts_api, React Aria Table, Vite SPA.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-10-contact-handles-table-design.md`
- Trash unlinks `contact_handles` only
- Date range `YYYY-MM-DD`; conversations/messages as two stacked lines

---

Implemented as part of the 2026-08-10 contact handles table work. See the design spec for behavior.
