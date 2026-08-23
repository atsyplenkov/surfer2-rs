# 02 — Restore public skeleton completion for the strict corpus

**What to build:** Make the strict convex, spoke, iso-simple, degree-one, and weighted corpus gates complete as validated skeletons through the supported public API while preserving their frozen kinetic and output counts.

**Blocked by:** 01 — Expose actionable bounded-cycle failures.

**Status:** done

- [x] Every strict corpus count gate obtains its result through the public skeleton API and treats incomplete bounded coverage as a test failure.
- [x] The existing frozen counts remain unchanged for every covered corpus fixture.
- [x] No corpus assertion substitutes a duplicated raw-arc count for successful DCEL construction.
- [x] The relevant release corpus checks pass.
