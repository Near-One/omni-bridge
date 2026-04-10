# Omni Relayer

Off-chain component of [Omni Bridge](https://github.com/near-one/omni-bridge) that relays transfers between NEAR and other networks (Ethereum, Solana, BTC, and more).

## Deployment

Docker Compose is the recommended way to deploy the relayer. It bundles the relayer with NATS (message queue) and Redis (state store), and automatically creates the required JetStream streams.

### 1. Configure secrets

```bash
cp .example-env .env
```

Edit `.env` and fill in your credentials:

| Variable | Required | Description |
|----------|----------|-------------|
| `NEAR_OMNI_ACCOUNT_ID` / `NEAR_OMNI_PRIVATE_KEY` | Yes | NEAR account for signing relay transactions |
| `ETH_PRIVATE_KEY`, `BASE_PRIVATE_KEY`, ... | Per chain | EVM chain private keys (only for chains you enable) |
| `SOLANA_PRIVATE_KEY` | If Solana enabled | Solana keypair (bs58-encoded) |
| `INFURA_API_KEY` | If using Infura | API key for EVM RPC endpoints |
| `MONGODB_USERNAME` / `MONGODB_PASSWORD` / `MONGODB_HOST` | If using bridge indexer | Bridge indexer database credentials |
| `BRIDGE_NATS_USERNAME` / `BRIDGE_NATS_PASSWORD` | Yes | NATS authentication credentials |

### 2. Configure the relayer

```bash
cp example-docker-config.toml config.toml
```

Edit `config.toml`:
- **Enable only the chains you want to relay** — comment out or remove sections for chains you don't support
- **Set your RPC endpoints** — replace Infura URLs with your own providers if preferred
- **Adjust fee settings** — `fee_discount` in `[bridge_indexer]` controls how much discount you accept (0-100)
- **Token whitelist** (optional) — restrict to specific tokens via `whitelisted_tokens`

### 3. Set NATS credentials

The included `nats.conf` reads credentials from environment variables passed by docker-compose. Set `BRIDGE_NATS_USERNAME` and `BRIDGE_NATS_PASSWORD` in your `.env` file — these are used for both the NATS server and the relayer connection.

### 4. SSH access for build

The build fetches private GitHub dependencies via SSH. Make sure your SSH agent has a key with access to `github.com/near-one`:

```bash
ssh-add -l                    # check loaded keys
ssh-add ~/.ssh/id_ed25519     # add if needed
ssh -T git@github.com         # verify access
```

### 5. Deploy

```bash
docker compose up -d
```

This starts:
- **NATS** — JetStream message queue
- **nats-init** — creates `OMNI_EVENTS` and `RELAYER` streams (runs once, idempotent)
- **Redis** — state and checkpoint storage
- **relayer** — the omni-relayer process

All services are configured with `restart: unless-stopped` for automatic recovery.

### Verify

```bash
# Check all services are running
docker compose ps

# View relayer logs
docker compose logs -f relayer

# Check NATS streams were created
docker compose logs nats-init
```

### Update

```bash
git pull
docker compose up -d --build
```

## Configuration Reference

The relayer is configured through a TOML file with environment variable substitution at parse time (variables like `INFURA_API_KEY` in RPC URLs are replaced automatically from the environment).

Example configs:
- `example-docker-config.toml` — docker-compose deployment (recommended starting point)
- `example-devnet-config.toml` — devnet/testnet with all chains
- `example-testnet-config.toml` — testnet
- `example-mainnet-config.toml` — mainnet

| Section | Purpose |
|---------|---------|
| `[redis]` | Connection URL and retry settings |
| `[nats]` | Connection URL, consumer names, retry backoff, worker count |
| `[bridge_indexer]` | Bridge API URL, MongoDB, fee discount, token whitelist |
| `[near]` | NEAR RPC, bridge contract IDs, signer credentials |
| `[eth]`, `[base]`, `[arb]`, `[bnb]`, `[pol]` | Per-chain RPC URLs, bridge addresses, finalization settings |
| `[solana]` | Solana RPC, program IDs, discriminators |
| `[btc]`, `[zcash]` | UTXO chain RPC and light client settings |
| `[eth.fee_bumping]` | EVM transaction fee bumping thresholds |

## Architecture

```
Indexers (NEAR / EVM / Solana / MongoDB)
  └─► NATS OMNI_EVENTS stream
        └─► Bridge Indexer consumer
              └─► NATS RELAYER stream
                    └─► Worker pool (process_events)
                          └─► OmniConnector SDK ─► destination chain
```

1. **Indexers** watch source chains for bridge events
2. Events flow through NATS JetStream with at-least-once delivery
3. **Workers** validate fees, build proofs, and finalize transfers via the OmniConnector SDK
4. **Fee bumping** monitors pending EVM transactions and resubmits with higher gas when stuck

## Building from Source

If you prefer to run without Docker:

```bash
# Prerequisites: Rust 1.86+, running Redis, running NATS with JetStream

# Create JetStream streams (one-time setup)
nats stream add OMNI_EVENTS --subjects "omni-events.>" --retention limits --storage file --discard old
nats stream add RELAYER --subjects "relayer.tasks.>" --retention limits --storage file --discard old

# Build and run
cp .example-env .env
cp example-devnet-config.toml config.toml
# Edit .env and config.toml

cargo run -- --config config.toml
```
