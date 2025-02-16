# Omni Bridge End-to-End Tests

## General description

The E2E tests cover an entire workflow involving multiple blockchain components (NEAR, EVM-based chains, Solana) and cross-chain communication. These tests ensure that all parts (smart contracts, scripts, etc.) integrate correctly. The Makefiles in this project orchestrate each step in the workflow, from compiling and deploying contracts on various chains to executing and verifying cross-chain transactions.

## Prerequisites

You will need the following tools installed on your environment before proceeding:
- **Yarn**. Used for installing TypeScript dependencies for EVM contracts and scripts.
- **Cargo**. The Rust package manager, required for building Rust-based components.
- **NEAR CLI RS**. A command-line interface for interacting with NEAR protocols.
- **Docker**. Required to build NEAR contracts in consistent environment.
- **Solana CLI and Anchor**. For compiling and deploying Solana programs.
- **Bridge SDK CLI**. Install with:
`cargo install --git https://github.com/Near-One/bridge-sdk-rs/ --rev b7c5acf bridge-cli`
- **Cargo Near**. Used to build NEAR contracts. Install with:
`cargo install --locked cargo-near`
Enables bridging functionality for various blockchain environments.
- **jq**. A command-line JSON processor used by many scripts in this project.

## Project Structure

The E2E testing project is organized into several key directories:

### Main Directories
- `tools/` - TypeScript-based utilities and scripts that support the E2E testing process:
  - `src/lib/` - Contains shared libraries and utilities used across different scripts
  - `src/scripts/` - Contains the main scripts for deployment, testing, and chain interaction
  - `src/E2ETestToken/` - Contains token-related implementations and utilities

- `makefiles/` - Contains modular Makefiles split by functionality:
  - `near.mk`, `evm.mk`, `solana.mk` - Chain-specific build and deployment rules
  - `pipelines/` - Complex multi-chain testing scenarios (e.g., `bridge_token_near_to_evm.mk`)
  - `common.mk` - Shared variables and utility functions

- `generated/` - **All generated files and artifacts are stored here**, including:
  - Build outputs
  - Deployment results
  - Test artifacts
  - Transaction receipts
  - Contract addresses
  This directory is automatically created and managed by the build system.

- `bin/` - Contains compiled binaries and executables used by the testing process

### Configuration Files
- `bridge-sdk-config.example.json` - Template for bridge SDK configuration
- `near_init_params.json` - NEAR contract initialization parameters
- Various keypair files for Solana program deployment

## User guide

### How to Run Builds and Pipelines

This repository contains multiple Makefiles, each focusing on a particular chain or pipeline.

You can explore all the available targets by running:
```
make help
```

Typical usage involves calling a specific pipeline target, for example:

```
make bridge-token-near-to-evm
```

This command triggers a multi-step process that compiles, deploys, and binds tokens across NEAR and an EVM-based network.

### Environment variables and configuration

You need to create a `.env` files from the examples provided in:
- `./tools/.env.example`
- `../evm/.env.example` (`INFURA_API_KEY` and `EVM_PRIVATE_KEY` only)

Also you need to copy or rename the provided `bridge-sdk-config.example.json` to `bridge-sdk-config.json`. And update it with your `ETH_PRIVATE_KEY` and your `ETH_RPC` endpoint.

For Solana bulding and deployment, ensure that for every program you have a keypair in `.e2e-testing/` directory in the format of `<program_name>-keypair.json`. However, this key pair is secret and should not be shared.

### Result artifacts

Throughout the pipelines, you will see JSON files containing addresses, transaction hashes, and other relevant data.
These files serve as evidence that each step or deployment was successfully executed. They are automatically generated and stored in dedicated directories such as `evm_deploy_results` or `near_deploy_results`.

### Handling Pipeline Failures

If a pipeline fails at a certain step, fix the underlying issue and rerun the same target. Make will pick up from the point of failure if the previous steps have created their artifact files or “stamp” files.

### Rebuilding Binaries

Each build step depends on “stamp” files that mark the completion of the step. Simply calling the relevant build target again will skip the build if the stamp file exists.

To perform a clean rebuild, run the corresponding clean target. For example:
```
  make clean-near
  make near-build
```

This removes the old artifacts and stamps, forcing a complete recompile.

## Developer Guide

### Introduction to Make

- If you are new to Make, the [[GNU Make Manual](https://www.gnu.org/software/make/manual/make.html)] is an excellent place to start.
- Make allows us to define rules that specify how to build or process files and manage dependencies.


### Structure of the Makefiles

- Makefiles here are split into modules for different chains (e.g., `near.mk`, `evm.mk`, `solana.mk`) and pipelines (e.g., `pipelines/bridge_token_near_to_evm.mk`).  
- Each of these is included into a master Makefile.
- Variables are in a global namespace. Therefore, every variable should be prefixed to avoid naming collisions, such as `evm_compile_stamp`, `solana_build_stamp`, etc.

### Pipelines Organization

- Each pipeline typically has a prefix, such as pipeline1, pipeline2, and so on.
- Consider adding new pipelines in separate `.mk` files.
- Number the generated files for clarity (e.g., `01_step.json`, `02_step.json`) to keep track of the pipeline steps.

### Targets and Phony Targets

Most steps have two targets:

1. A “file” target (e.g., a JSON artifact) or a “stamp” file target to indicate completion.
2. A `.PHONY` target to run that step directly.

The phony target typically depends on the file target, ensuring that the command is performed when necessary.

Each step usually prints a brief description before execution using a helper function like `description`, so you know what is happening.

### Order of prerequsites

In certain cases, the order in which prerequisites are listed (and thus passed to scripts) can matter. Pay special attention to the scripts that rely on positional arguments.

### Special targets

- Every module or feature should provide a custom `clean-{custom-name}` target that removes the artifacts and stamp files it generates.
- Remember to add that clean target to the help target or a consolidated list of “clean” targets so that users can discover and run it easily.
- After adding module, don't forget to add it to the `help` target.

### Debugging

- Run `make --dry-run` to print the commands that would be executed without actually running them.
- Use `make  -d` for a more verbose explanation of why each command runs (or doesn’t run). It could be used with `make --dry-run` in order not to run the commands.

## General recommendations for Makefiles in this project

Below are some guidelines to maintain consistency and clarity:
1. Internal variables (not environment or inherited from parent Makefile) should use lowercase.
2. Prefer `:=` (immediate assignment) over `=` (delayed assignment) for most variable definitions.
3. Use `.INTERMEDIATE` and `.PHONY` to define targets properly.
4. Make each phony target a prerequisite of `.PHONY` right before declaring the target.
5. Avoid using phony targets as prerequisites for file targets.
6. When a Makefile grows large, consider splitting it into multiple files, each handling a distinct set of tasks or a single pipeline.
7. Use automatic variables like `$^`, `$@`, `$<`, and `$(word n,$^)` wisely to simplify commands.
8. Directories often work best as order-only prerequisites (using the pipe symbol `|`) to avoid rebuilding them unnecessarily.
9. Try to avoid recursive Make patterns; a single dependency graph is often clearer.
10. Control verbosity to suit the needs of the project, for example by hiding command echoes and only printing the essential logs.

