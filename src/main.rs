//! Binary entry point; all logic lives in the library crate.

fn main() -> std::process::ExitCode {
    multithreaded_data_transfer_rs::cli::main()
}
