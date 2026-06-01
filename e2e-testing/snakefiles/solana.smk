import pathlib
import const

configfile: "config.yaml"

module common_module:
    snakefile: "common.smk"

use rule * from common_module as common_*

# Solana-specific variables and paths
solana_dir = const.common_testing_root / "../solana"
solana_build_stamp = const.common_generated_dir / ".solana-build.stamp"
solana_artifacts_dir = const.common_generated_dir / "solana_artifacts"

# Programs to build
solana_programs = ["bridge_token_factory"]

def get_program_binary_path(program_name, solana_root):
    return f"{solana_root}/target/verifiable/{program_name}.so"

def get_mkdir_cmd(directory):
    return f"mkdir -p {directory}"

# Rule to build all Solana programs
rule solana_build:
    input:
        binaries=expand(get_program_binary_path("{program}", solana_artifacts_dir), program=solana_programs)
    message: "Building Solana programs"

# Rules for each program's keypair and binary
for program in solana_programs:
    rule:
        name: f"build_{program}_program"
        output:
            binary=get_program_binary_path("{program}", solana_dir)
        message: f"Building Solana program {program}"
        params:
            mkdir=get_mkdir_cmd(str(solana_artifacts_dir / program)),
            program_dir=solana_dir / program
        shell: """
        {params.mkdir} && \
        cd {params.program_dir} && \
        RUSTUP_TOOLCHAIN="nightly-2024-11-19" anchor build --verifiable && \
        cp -r {params.program_dir}/target/ {solana_artifacts_dir}/{program}
        """
