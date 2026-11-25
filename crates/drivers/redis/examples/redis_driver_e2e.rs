//! Standalone entry point for the live Redis driver fixture.
#[path = "redis_driver_e2e/fixture.rs"]
mod fixture;

fn main() {
    fixture::run_cli(std::env::args().skip(1));
}
