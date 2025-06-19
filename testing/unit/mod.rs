// Unit tests are split into separate modules to prevent OOM issues
// when compiling many async tests in a single file

mod basic_tests;
mod config_tests;
mod debug_tests;
mod error_handling_tests;
mod move_detector_tests;
mod real_world_move_tests; // New real-world integration tests
mod stress_tests;
mod windows_integration_tests;
mod windows_specific_tests;
