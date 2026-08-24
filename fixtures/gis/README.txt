# GIS conformance fixtures

The input geometries and frozen C++ OBJ oracles are vendored here. Tests use
this directory by default; set `SURFER2_GIS_DATA` to override it.

- `lake9706-cpp-double.obj`: Surfer2 `c42f9a7`, `NT_USE_DOUBLE=ON`, interior
  component (`--component=1`); 4372 finite non-zero segments, 4344 unique XY
  nodes, total length 342587.19566660019.
- `lake10623-cpp-exact.obj`: Surfer2 `c42f9a7`, exact kernel, interior component
  (`--component=r`, equivalent to `--component=1`); 31330 finite non-zero
  segments, 31130 unique XY nodes, total length 1007737.3828873356.

The exact oracle is used for lake10623 because the historical C++ double build
completed propagation but crashed while constructing the final DCEL and wrote
an empty OBJ. Oracle OBJ output was written with stream precision 17.

## Rebuilding the C++ oracle binaries

Set `SURFER2_CPP_SOURCE` to a checkout of cgalab/surfer2 at revision
`c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`:

```bash
cmake -S "$SURFER2_CPP_SOURCE" -B /tmp/surfer2-build-double \
  -DCMAKE_BUILD_TYPE=Release -DNT_USE_DOUBLE=ON -DDEBUG_OUTPUT=OFF
cmake --build /tmp/surfer2-build-double --target surfer

cmake -S "$SURFER2_CPP_SOURCE" -B /tmp/surfer2-build-exact \
  -DCMAKE_BUILD_TYPE=Release -DNT_USE_DOUBLE=OFF -DDEBUG_OUTPUT=OFF
cmake --build /tmp/surfer2-build-exact --target surfer
```

The historical CGAL 6 build disabled the Qt GUI subdirectory and needed minor
CGAL compatibility patches; those local build patches are not part of this
repository. The C++ executable consumes GraphML, so regenerate equivalent
GraphML from the vendored JSON before replacing an oracle. Preserve 17-digit
OBJ precision.

Optional differential tests discover binaries through `SURFER2_DOUBLE_BIN` and
`SURFER2_EXACT_BIN`.

## Producing C++ offset oracles

No C++ offset oracles are vendored today. Offset gates in `tests/gis.rs` are
named property / API-contract smokes only; broader C++ offset conformance is
**blocked-on-oracle-generation** until artifacts from this procedure land.

Rebuild the double (and optionally exact) binaries as in “Rebuilding the C++
oracle binaries” above — same Surfer2 revision
`c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`, `NT_USE_DOUBLE=ON` (or OFF for
exact), `DEBUG_OUTPUT=OFF`.

Offsets are drawn into the IPE `offsets` layer. Plain OBJ output is skeleton
faces only and does **not** embed offset polylines, so use `--ipe`:

```bash
# GraphML already vendored under fixtures/graphml/, or emit from bits:
#   GraphMlInput::from_bits_record(...).to_graphml()  (tests/support/graphml.rs)
#   writes 17-digit vertex coordinates.

/tmp/surfer2-build-double/cc/surfer \
  --component=-1 \
  --sk-offset='0.1+0.2' \
  --ipe \
  fixtures/graphml/convex03.graphml \
  /tmp/convex03-offsets.ipe
```

`--sk-offset` grammar (from the binary help): comma-separated blocks,
`+`-joined times within a block, optional `cnt*time` repeats — e.g.
`0.01 + 3*0.025, 0.15` or `10 * 0.025`.

Vendor the resulting IPE (or a parsed polyline extract) under
`fixtures/oracle/offset/` using names that encode geometry, kernel, and
offset-spec (e.g. `convex03-double-t0.1.ipe`). Preserve full stream
precision (match the 17-digit OBJ practice when controlling formatting).
After vendoring, add an always-on comparison gate in the same change and
narrow the README Status “blocked-on-oracle-generation” note accordingly.
