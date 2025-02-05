# Solana-specific variables and rules
solana_dir := $(common_testing_root)/../solana

solana_build_stamp := $(common_generated_dir)/.solana-build.stamp
solana_artifacts_dir := $(common_generated_dir)/solana_artifacts

solana_programs := bridge_token_factory
solana_programs_keypairs := $(foreach program,$(solana_programs),$(solana_dir)/$(program)/target/deploy/$(program)-keypair.json)
solana_programs_binaries := $(foreach program,$(solana_programs),$(solana_artifacts_dir)/$(program)/target/deploy/$(program).so)

# Clean targets
.PHONY: clean-solana
clean-solana:
	$(call description,Cleaning Solana build artifacts)
	rm -rf $(solana_artifacts_dir)
	rm -f $(solana_build_stamp)

# Main build target
.PHONY: solana-build
solana-build: $(solana_build_stamp)
$(solana_build_stamp): $(solana_programs_keypairs) $(solana_programs_binaries)
	$(call description,Solana build complete)
	touch $@

# Program-specific build rules
define generate_solana_build_rules

$(solana_dir)/$(1)/target/deploy/$(1)-keypair.json: $(common_testing_root)/$(1)-keypair.json
	$(call description,Setting up keypair for $(1))
	mkdir -p $$(dir $$@) && \
	cp $$< $$@

$(solana_artifacts_dir)/$(1)/target/deploy/$(1).so: $(solana_dir)/$(1)/target/deploy/$(1)-keypair.json
	$(call description,Building Solana program $(1))
	mkdir -p $(solana_artifacts_dir)/$(1) && \
	cd $(solana_dir)/$(1) && \
	anchor build && \
	cp -r $(solana_dir)/$(1)/target/* $(solana_artifacts_dir)/$(1)

endef

$(foreach program,$(solana_programs),$(eval $(call generate_solana_build_rules,$(program))))