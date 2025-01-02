#!/usr/bin/env bash

# Exit script as soon as a command fails.
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
DEFAULT_RES_DIR="$DIR/res"
RES_DIR=$DEFAULT_RES_DIR

show_help() {
    echo "Usage: $0 [OPTION] [COMPONENT_DIR]"
    echo
    echo "Build NEAR smart contracts."
    echo
    echo "Options:"
    echo "  -h, --help              Display this help message"
    echo "  -o, --output-dir DIR    Specify output directory for WASM files (default: ./res)"
    echo
    echo "Arguments:"
    echo "  COMPONENT_DIR  Optional. Directory name of the specific component to build"
    echo "                 If not provided, builds all components"
    echo
    echo "Examples:"
    echo "  $0                   # Build all contracts"
    echo "  $0 omni-bridge       # Build only omni-bridge contract"
    echo "  $0 token-deployer    # Build only token-deployer contract"
    echo "  $0 -o ../dist        # Build all contracts and output to ../dist"
    echo
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        -o|--output-dir)
            if [ -z "$2" ]; then
                echo "Error: Output directory not specified"
                exit 1
            fi
            RES_DIR="$2"
            shift 2
            ;;
        *)
            COMPONENT="$1"
            shift
            ;;
    esac
done

# Determine Docker user flags
if [[ -z "$BUILDKITE" ]] && [[ "$(uname -s)" != "Darwin" ]];
then
     userflag="-u $UID:$UID"
else
     userflag=""
fi

# Determine architecture-specific tag
arch=`uname -m`
if [ "$arch" == "arm64" ]
then
    tag=":latest-arm64"
else
    tag=""
fi

if [ -z "$COMPONENT" ]; then
    BUILD_CMD="cargo build --workspace --target wasm32-unknown-unknown --release"
    echo "Building entire workspace..."
else
    if [ ! -d "$DIR/$COMPONENT" ]; then
        echo "Error: Directory '$COMPONENT' not found in $DIR"
        exit 1
    fi
    BUILD_CMD="cargo build --manifest-path $COMPONENT/Cargo.toml --target wasm32-unknown-unknown --release"
    echo "Building component: $COMPONENT"
fi

docker run \
     --rm \
     --mount type=bind,source=$DIR,target=/host \
     --cap-add=SYS_PTRACE --security-opt seccomp=unconfined $userflag \
     -w /host \
     -e RUSTFLAGS='-C link-arg=-s' \
     nearprotocol/contract-builder$tag \
     /bin/bash -c "rustup target add wasm32-unknown-unknown && $BUILD_CMD"

mkdir -p $RES_DIR

if [ -z "$COMPONENT" ]; then
    find $DIR/target/wasm32-unknown-unknown/release/ -name "*.wasm" -exec cp -f {} $RES_DIR/ \;
else
    binary_name=$(basename $COMPONENT | tr '-' '_')
    find $DIR/target/wasm32-unknown-unknown/release/ -name "$binary_name.wasm" -exec cp -f {} $RES_DIR/ \;
fi

echo "Build completed! Contract files are in the $RES_DIR directory:"
ls -l $RES_DIR