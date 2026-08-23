# 01 — Expose actionable bounded-cycle failures

**What to build:** Make strict DCEL construction explain exactly why bounded coverage could not be completed, so subsequent fixture repairs target the broken topology rather than bypassing validation.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] A failed bounded-cycle build identifies the affected face, halfedge or walk endpoint, and the cycle-construction stage that failed.
- [x] Face-walk failures remain available in the returned diagnostic instead of being silently discarded.
- [x] Existing inputs that already produce valid DCELs retain their segment topology and pass the relevant project checks.
