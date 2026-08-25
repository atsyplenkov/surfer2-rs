// Shared helpers compiled separately into each test binary; every target only
// uses part of them, so dead_code is expected here.
#[allow(dead_code)]
pub mod graph_match;
#[allow(dead_code)]
pub mod graphml;
#[allow(dead_code)]
pub mod oracle;
#[allow(dead_code)]
pub mod skip_inf;
