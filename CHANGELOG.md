# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

### Added

- Headless straight-skeleton engine ported from [cgalab/surfer2](https://github.com/cgalab/surfer2) (revision `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`): kinetic triangulation and wavefront propagation. 
- `interior_straight_skeleton(polygon)` and `straight_skeleton(pslg, options)` entry points, plus `offset(skeleton, time)` for offset contours at any
  wavefront time.
- Polygons with holes (`Polygon`) and general planar straight-line graphs   (`Pslg`) as input, with multiplicative edge weights and per-component propagation modes.
- Frozen C++ oracle conformance gates (`tests/gis.rs`) covering the Magadan lake fixtures and the upstream corpus.
- Criterion benchmarks (`benches/magadan.rs`).
