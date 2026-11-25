use embed_manifest::{embed_manifest, manifest::ExecutionLevel, new_manifest};

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest(
            new_manifest("VG_Switcher").requested_execution_level(ExecutionLevel::RequireAdministrator),
        )
        .expect("unable to embed manifest");

        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("unable to embed icon resource");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
