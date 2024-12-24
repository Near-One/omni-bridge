# End-to-end testing

## Prerequisites

- yarn
- cargo
- [NEAR CLI RS](https://github.com/near/near-cli-rs)
- docker
- [Solana CLI and Anchor](https://solana.com/docs/intro/installation)
- Bridge SDK CLI: `cargo install  --git https://github.com/Near-One/bridge-sdk-rs/ bridge-cli`

## Using the Makefile

The `Makefile` in this project is designed to automate the deployment and compilation processes for both EVM and NEAR environments. It provides a set of predefined rules that can be executed to perform specific tasks, such as compiling contracts, deploying them to various networks, and setting up necessary infrastructure.

### Common Tasks

- **Compile EVM Contracts**: To compile the EVM contracts, run:
  ```bash
  make evm-compile
  ```

- **Deploy to EVM Networks**: To deploy contracts to a specific EVM network, use the following pattern:
  ```bash
  make <network>-deploy
  ```
  Replace `<network>` with the desired network name, such as `sepolia` or `arbitrumSepolia`.

- **Build NEAR Contracts**: To build the NEAR contracts, execute:
  ```bash
  make near-build
  ```

- **Deploy NEAR Contracts**: To deploy NEAR contracts, run:
  ```bash
  make near-deploy
  ```

These tasks automate the process of setting up the testing environment, ensuring that all necessary components are compiled and deployed correctly.

### Additional Requirements

- **Private Key Requirement**: For Ethereum deployment, ensure that you add your `EVM_PRIVATE_KEY` to the `./evm/.env` file. This key is necessary for authenticating transactions on the Ethereum network.

- **Solana Keypair Requirement**: For Solana bulding and deployment, ensure that for every program you have a keypair in `.e2e-testing/` directory in the format of `<program_name>-keypair.json`. However, this key pair is secret and should not be shared.

### Deployment Results

- **Storage of Results**: The addresses of deployed contracts are stored in JSON files in their corresponding locations. For example, the token factory deployed on the `arbitrumSepolia` network will be stored in `evm_deploy_results/arbitrumSepolia/token_factory.json`. This is done to reuse these addresses across different runs of the tests.

### General Working Principles

- **Phony Targets**: The `Makefile` uses `.PHONY` targets to define tasks that do not correspond to actual files. This ensures that these tasks are always executed when called, regardless of the presence of files with the same name.

- **Variables**: The `Makefile` defines several variables to manage paths and configurations, such as `TESTING_ROOT`, `EVM_DIR`, and `NEAR_DIR`. These variables help in organizing the file structure and making the `Makefile` adaptable to different environments.

- **Rule Expansion**: The `Makefile` uses a combination of static and dynamic rule definitions. Dynamic rules are generated using the `define` directive, which allows for the creation of rules based on the networks specified in the `EVM_NETWORKS` variable. This approach reduces redundancy and makes it easy to add support for additional networks.

- **Dependencies**: Each target in the `Makefile` specifies its dependencies, ensuring that all necessary steps are completed before executing a task. For example, deploying a contract requires that it is compiled first.

- **Command Execution**: The `Makefile` uses shell commands to execute tasks, such as running `yarn` for EVM contract compilation and deployment, and custom scripts for NEAR contract deployment.

In order to see which commands will be executed without actually executing them, you can add `--dry-run` to the command.