use std::str::FromStr;

use cargo_near_build::BuildOpts;
use cargo_near_build::{bon, camino, extended};

fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let workdir = "../omni-token";
    let nep330_contract_path = "near/omni-token";

    let manifest = camino::Utf8PathBuf::from_str(workdir)
        .expect("pathbuf from str")
        .join("Cargo.toml");

    let build_opts = BuildOpts::builder()
        .manifest_path(manifest)
        .no_locked(true)
        .override_nep330_contract_path(nep330_contract_path)
        .override_cargo_target_dir("../target/build-rs-omni-token")
        .build();

    let build_script_opts = extended::BuildScriptOpts::builder()
        .rerun_if_changed_list(bon::vec![workdir, "Cargo.toml", "../Cargo.lock"])
        .build_skipped_when_env_is(vec![(
            cargo_near_build::env_keys::BUILD_RS_ABI_STEP_HINT,
            "true",
        )])
        .stub_path("../target/omni-token.bin")
        .result_env_key("BUILD_RS_SUB_BUILD_OMNI-TOKEN")
        .build();

    let extended_opts = extended::BuildOptsExtended::builder()
        .build_opts(build_opts)
        .build_script_opts(build_script_opts)
        .build();

    cargo_near_build::extended::build(extended_opts)?;
    Ok(())
}
