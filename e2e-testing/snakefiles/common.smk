import const

rule tools_build:
    output: const.common_tools_compile_stamp
    message: "Building tools"
    shell: """
    yarn --cwd {const.common_tools_dir} install && \
    yarn --cwd {const.common_tools_dir} hardhat compile
    touch {output}
    """



