pub fn should_update_goldens(suite_env_var: &str) -> bool {
    std::env::var_os("UPDATE").is_some() || std::env::var_os(suite_env_var).is_some()
}
