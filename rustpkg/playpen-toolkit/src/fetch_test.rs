use super::*;

#[test]
fn fetch_option_schema() {
    let schema = schemars::schema_for!(FetchOption);
    let json = serde_json::to_string_pretty(&schema).unwrap();
    assert!(json.contains("url"));
}

#[test]
fn fetch_error_display() {
    assert_eq!(format!("{}", FetchError::HttpStatus("404".into())), "404");
    assert_eq!(format!("{}", FetchError::Timeout("超时".into())), "超时");
}
