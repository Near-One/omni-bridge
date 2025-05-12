import json


def get_json_field(json_file, field):
    # Conversion to string is necessary to avoid issues with snakemake.io.Namedlist which can come as input
    with open(str(json_file)) as f:
        return json.load(f)[field]


def progress_wait(seconds):
    """
    Generates a simple progress bar command that waits for the specified seconds.
    It displays completion percentage, elapsed time, and remaining time.
    """
    return f"""
    for i in $(seq 1 {seconds}); do \\
        if [ $((i % 30)) -eq 0 ] || [ $i -eq 1 ] || [ $i -eq {seconds} ]; then \\
            remaining_secs=$(( {seconds} - i )); \\
            remaining_mins=$((remaining_secs / 60)); \\
            elapsed_secs=$i; \\
            elapsed_mins=$((elapsed_secs / 60)); \\
            printf "\\r%d%% | Elapsed: %dm %ds | Remaining: %dm %ds" \\
                "$((i * 100 / {seconds}))" \\
                "$elapsed_mins" \\
                "$((elapsed_secs % 60))" \\
                "$remaining_mins" \\
                "$((remaining_secs % 60))"; \\
        fi; \\
        sleep 1; \\
    done; \\
    printf '\\n'
    """


def extract_tx_hash(pattern_type, output_file):
    """
    Generates a shell command to extract transaction hash from command output and format as JSON.

    Parameters:
        pattern_type: The type of output pattern to match:
            - "near": Matches 'Transaction ID: HASH' pattern (for near-cli contract calls)
            - "bridge": Matches 'tx_hash="HASH"' pattern (for bridge-cli calls)

    Returns:
        Shell command string that extracts the hash and writes it to the output file
    """
    if pattern_type == "near":
        return f"""
    TX_HASH=$(grep -o 'Transaction ID: [^ ]*' {output_file} | cut -d' ' -f3) && \\
    echo '{{\"tx_hash\": \"'$TX_HASH'\"}}' > {output_file}
    """
    elif pattern_type == "bridge":
        return f"""
    TX_HASH=$(grep -o 'tx_hash="[^"]*"' {output_file} | cut -d'"' -f2) && \\
    echo '{{\"tx_hash\": \"'$TX_HASH'\"}}' > {output_file}
    """
    else:
        raise ValueError(f"Unknown pattern type: {pattern_type}")


def get_mkdir_cmd(directory):
    return f"mkdir -p {directory}"
