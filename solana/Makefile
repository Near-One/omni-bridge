.PHONY: upgrade-fogo

KEYPAIR ?= $(HOME)/.config/solana/id.json

# Upgrade the FOGO bridge program via the Squads V3 multisig. Runs three steps:
#   1) Write the new .so to a buffer on FOGO (paid by KEYPAIR).
#   2) Transfer buffer authority to the multisig vault.
#   3) Print a base58 Solana Message containing the BPF Loader Upgradeable
#      `Upgrade` instruction. A multisig member pastes this into
#      `squads-cli --cluster <FOGO_RPC_URL>` → Create Transaction →
#      "Enter Transaction (base58 serialized message)". squads-cli wraps it
#      into a multisig tx and signs the create-tx call via the member's wallet
#      (Ledger or otherwise).
#
# Required: FOGO_RPC_URL, FOGO_MULTISIG (the multisig PDA).
# Optional: KEYPAIR (defaults to ~/.config/solana/id.json),
#           SPILL   (defaults to KEYPAIR's pubkey; pass to redirect the buffer's
#                    reclaimed rent to a different account — typically the
#                    wallet that originally paid for the buffer in step 1),
#           BUFFER  (skip steps 1+2 and only emit the base58 message — useful
#                    when steps 1+2 already succeeded but you want to retry
#                    step 3, e.g., to regenerate the message for a member).
#
# Invoke from the project root with `make -C solana upgrade-fogo`, or `cd solana`
# first. All paths in this Makefile are relative to the solana/ directory.
#
# Example:
#   make -C .. solana-build-fogo FOGO_PROGRAM_ID=<addr>
#   make -C solana upgrade-fogo \
#       FOGO_RPC_URL=https://mainnet.fogo.io \
#       FOGO_MULTISIG=<multisig-pda> \
#       SPILL=<buffer-payer-pubkey>
upgrade-fogo:
	@test -n "$(FOGO_RPC_URL)" || { echo "FOGO_RPC_URL is required" >&2; exit 1; }
	@test -n "$(FOGO_MULTISIG)" || { echo "FOGO_MULTISIG is required" >&2; exit 1; }
	@test -d scripts/node_modules || { echo "scripts deps missing — run \`cd scripts && npm install\`" >&2; exit 1; }
	@if [ -z "$(BUFFER)" ]; then test -f target/verifiable/bridge_token_factory_fogo.so || { echo "target/verifiable/bridge_token_factory_fogo.so not found — run \`make -C .. solana-build-fogo\` first, or pass BUFFER=<addr>" >&2; exit 1; }; fi
	VAULT=$$(cd scripts && FOGO_MULTISIG=$(FOGO_MULTISIG) npm run --silent upgrade-fogo -- --print-vault) && \
	echo "Vault: $$VAULT" && \
	if [ -n "$(BUFFER)" ]; then \
	  BUFFER=$(BUFFER); echo "Reusing buffer: $$BUFFER"; \
	else \
	  echo "Writing buffer..." && \
	  BUFFER=$$(solana program write-buffer target/verifiable/bridge_token_factory_fogo.so \
	    --url $(FOGO_RPC_URL) --keypair $(KEYPAIR) | tee /dev/stderr | awk '/^Buffer:/ {print $$2}') && \
	  test -n "$$BUFFER" || { echo "Failed to capture buffer address from solana program write-buffer" >&2; exit 1; } && \
	  echo "Transferring buffer authority to vault $$VAULT..." && \
	  solana program set-buffer-authority $$BUFFER \
	    --new-buffer-authority $$VAULT \
	    --url $(FOGO_RPC_URL) --keypair $(KEYPAIR); \
	fi && \
	echo "Generating base58 message for squads-cli..." && \
	cd scripts && \
	BUFFER=$$BUFFER FOGO_RPC_URL=$(FOGO_RPC_URL) FOGO_MULTISIG=$(FOGO_MULTISIG) KEYPAIR=$(KEYPAIR) \
	  $(if $(SPILL),SPILL=$(SPILL) ,)npm run --silent upgrade-fogo -- --print-message
