use playpen_config::AppConfig;

pub fn print_config(app: &AppConfig) {
    if let Ok(s) = toml::to_string_pretty(app) {
        println!("{}", s.trim_end());
    }
}
