BLOCKED

HEAD is still 1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c on fix/issue-1-continue. No commit was added. The done bar is not green, and the uncommitted diff is an experiment, not a fix. 1b6ed8e was not amended. Nothing was pushed and no PR was opened.

Log at 1b6ed8e: `fix: keep finite fast intersections on projected ridges`.

Release command on the current uncommitted tree (exit 101, 5.60s):

```
cargo test --release --test projected_polygons --locked -- basin_spoke_collapse_keeps_leftover_ridge projected_basin_1_has_connected_finite_skeleton projected_basin_2_is_one_tree indi_pol_river_keeps_connected_tree -- --test-threads=1 --nocapture
```

basin_spoke_collapse_keeps_leftover_ridge ok
indi_pol_river_keeps_connected_tree ok
projected_basin_1_has_connected_finite_skeleton FAILED: 58 components, edges=5679 nodes=5730 rank=7 (bounded validation passed)
projected_basin_2_is_one_tree FAILED: IncompleteFaceCycle stage=bounded-validation face=2268 owns 78 halfedges but has no root

Full `cargo test --release --locked` was not run. `cargo fmt --check` and clippy were not run. Debug basins were not re-run. No suite or clippy exit code.

Confirmed cause, from printed halfedges and the collapse that produces them. On 1b6ed8e, face 600's open ends are (463811.705, 6272413.627) and (463811.204, 6272413.631), 0.502 m apart, plus a 7.3 mm stub. That triangle's slow edge is nearly vertical (relative velocity about (0.017, 2.000)). The x-only edge time is 1.24e-6 s before the planar meet. The third vertex, speed about 4.05e5, is still 0.50 m short, so the 5 mm same-stop does not link the face. Face 13 includes the halfedge out to (613157, 13048303). That point is a moderate-speed normal velocity stored with the far intersection origin, so position_at(birth) misses the birth point by megametres. A separate 1-constraint split successor with |sin| about 5.6e-6 is Degenerate, its natural collapse is Never, and it integrates for hundreds of seconds.

Those three decisions are patched in the working tree only: birth replay is pinned when the miss exceeds 1 mm (the speed>1e6 pin stays; sub-millimetre replay keeps the historical origin), a 3-constraint triangle collapses only when the endpoints actually meet, a 1-constraint Degenerate successor may collapse now when the natural event is Never, and an edge collapse uses the planar closest-approach time when the residual is at most a centimetre. Spoke and the river counts stay put. Face 13 and face 600 go away. Feature 1 then has a valid DCEL and 58 coordinate components. The gaps have median length 51.67 m, the dominant input edge (92.8 m is the other). Feature 2's open ends near (437345, 6284236) and (437372, 6284235) are 27 m apart, with a 30 km self-loop on the same face.

The next wrong decision is a constraint-collapse child whose supporting lines are Degenerate (|sin| about 1.2e-5, speed about 1.65e5, opposing). On feature 1 it is born at t about 25.8, half of a 51.7 m corridor, already 92.8 m from the opposite vertex, and a later triangle collapse stops the pair an input edge apart. The river has four children of the same class that are real ridges (opposite vertex 3.2 m and 14.7 m away). Zeroing every fast Degenerate child splits the river into 4 components. Zeroing only the ones whose opposite vertex is more than 20 m away keeps the river and reopens feature 1's face cycle. Raising the same-time spoke repair from about 7 m to 200 m does the same. A finite fast intersection was not turned into a stationary vertex because its speed exceeded 1e6.

The velocity rule and the duplicate directed-halfedge drop from 1b6ed8e are still in the tree. docs/diagnostics/issue-1-projected-basins.md has this account in the working copy. No debug prints remain in src/.
