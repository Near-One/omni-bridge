use std::str::FromStr;

use cargo_near_build::{extended::BuildScriptOpts, BuildOpts};

fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    println!(
        "cargo:warning=`token-deployer` build script working dir: {:?}",
        std::env::current_dir().expect("get current dir")
    );

    let opts = cargo_near_build::extended::BuildOptsExtended {
        build_opts: BuildOpts {
            manifest_path: Some(
                cargo_near_build::camino::Utf8PathBuf::from_str("../omni-token/Cargo.toml")
                    .expect("camino PathBuf from str"),
            ),
            no_abi: true,
            ..Default::default()
        },
        build_script_opts: BuildScriptOpts {
            result_env_key: Some("OMNI_TOKEN_WASM".to_string()),
            rerun_if_changed_list: vec![
                "../omni-token".to_string(),
                "Cargo.toml".to_string(),
                "../Cargo.lock".to_string(),
            ],
            build_skipped_when_env_is: Vec::new().into(),
            stub_path: None,
        },
    };

    cargo_near_build::extended::build(opts)?;
    Ok(())
}
