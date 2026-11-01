#[cxx::bridge(namespace = "cxxzarlib")]
mod ffi {
    extern "Rust" {
        fn execute_json(operation: &str, request_json: &str) -> Result<String>;
    }
}

fn execute_json(operation: &str, request_json: &str) -> Result<String, String> {
    std::panic::catch_unwind(|| czarlib::execute_json(operation, request_json))
        .map_err(|_| "internal Rust panic".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::execute_json;

    #[test]
    fn reports_invalid_json() {
        let error = execute_json("info_archive", "{").unwrap_err();
        assert!(error.contains("invalid request JSON"));
    }

    #[test]
    fn reports_unknown_operation() {
        let error = execute_json("not_an_operation", "{}").unwrap_err();
        assert!(error.contains("unknown operation"));
    }
}
