use playpen_agent_core::tools::webfetch::WebfetchRigTool;
use rig_core::tool::Tool;

#[test]
fn webfetch_get_text() {
    let server = httpmock::MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/data");
        then.status(200)
            .header("Content-Type", "text/plain")
            .body("hello from web");
    });

    let tool = WebfetchRigTool;
    let rt = tokio::runtime::Runtime::new().unwrap();

    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "webfetch");

    let result = rt.block_on(tool.call(playpen_agent_core::tools::webfetch::WebfetchParams {
        url: server.url("/data"),
        timeout_ms: None,
        max_bytes: None,
    })).unwrap();
    assert!(result.contains("hello from web"));
    mock.assert_hits(1);
}

#[test]
fn webfetch_not_found() {
    let server = httpmock::MockServer::start();
    server.mock(|when, then| {
        when.method("GET").path("/missing");
        then.status(404);
    });

    let tool = WebfetchRigTool;
    let rt = tokio::runtime::Runtime::new().unwrap();

    let result = rt.block_on(tool.call(playpen_agent_core::tools::webfetch::WebfetchParams {
        url: server.url("/missing"),
        timeout_ms: None,
        max_bytes: None,
    }));
    assert!(result.is_err());
}
