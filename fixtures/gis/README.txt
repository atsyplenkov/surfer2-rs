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
