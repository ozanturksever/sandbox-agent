use std::fs;
use std::path::Path;

use sandbox_daemon_core::router::ApiDoc;
use utoipa::OpenApi;

fn main() {
    println!("cargo:rerun-if-changed=../sandbox-daemon/src/router.rs");
    println!("cargo:rerun-if-changed=../sandbox-daemon/src/lib.rs");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir).join("openapi.json");

    let openapi = ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&openapi)
        .expect("Failed to serialize OpenAPI spec");

    fs::write(&out_path, json).expect("Failed to write OpenAPI spec");
    println!("cargo:warning=Generated OpenAPI spec at {}", out_path.display());
}
