# 04 — Restore lake10623 with validated TraceAt topology

**What to build:** Make lake10623 return a fully validated skeleton directly from the supported public API, with TraceAt landings participating in reciprocal bounded face cycles and matching the frozen C++ exact oracle.

**Blocked by:** 01 — Expose actionable bounded-cycle failures.

**Status:** done

- [x] Lake10623 completes through the public skeleton API without an incomplete-face-cycle error or fallback emitter.
- [x] TraceAt ridge pieces and the landing spoke share one typed node and participate in reciprocal bounded face cycles.
- [x] Exact C++ oracle counts (31330 segments / 31130 nodes), length, cyclomatic 0, and topology match are asserted explicitly.
- [x] Length, cyclomatic number, proximity thresholds, and normalized topology comparison remain green.
- [x] The targeted release GIS checks for lake10623 pass.

**Resolution notes:** Near-degenerate TraceAt landings (within 1 mm of the host ridge Stop) share the Stop node instead of emitting a micro ridge piece. Leftover spokes take both of the child's incident wavefront faces so the twin participates in bounded cycles. Lake10623 then matches the C++ exact oracle exactly (31330 / 31130, cyclomatic 0, topology).
