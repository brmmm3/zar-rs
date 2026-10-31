/// Execute a pyzarlib-compatible archive operation with JSON input and output.
///
/// File paths are resolved by the WASI runtime and must be available through
/// its preopened directories.
pub fn execute_json(operation: &str, request_json: &str) -> Result<String, String> {
    czarlib::execute_json(operation, request_json)
}

#[cfg(target_os = "wasi")]
mod component {
    wit_bindgen::generate!({
        path: "wit",
        world: "wasizarlib",
    });

    struct Wasizarlib;

    impl exports::zar::wasizarlib::operations::Guest for Wasizarlib {
        fn execute(operation: String, request_json: String) -> Result<String, String> {
            super::execute_json(&operation, &request_json)
        }
    }

    export!(Wasizarlib);
}

#[cfg(test)]
mod tests {
    use super::execute_json;

    #[test]
    fn execute_json_reports_invalid_requests() {
        assert!(
            execute_json("check_archive", "not-json")
                .unwrap_err()
                .starts_with("invalid request JSON:")
        );
    }
}
