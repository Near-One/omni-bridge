use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let program_id =
        env::var("PROGRAM_ID").unwrap_or("3ZtEZ8xABFbUr4c1FVpXbQiVdqv4vwhvfCc8HMmhEeua".into());

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("program_id.rs");
    fs::write(&dest_path, format!("declare_id!(\"{}\");", program_id)).unwrap();
}
