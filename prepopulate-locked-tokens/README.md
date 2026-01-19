# Prepopulate tokens

This is needed as a first step for the contract after migration to set initial values of `locked_tokens`

## How to get a list of NEP141 tokens

```mongodb
db.omni_events.distinct(
  "Meta.NearBindTokenEvent.token_id",
  { "Meta.NearBindTokenEvent": { $exists: true } }
)
```

## How to get a list of other tokens

```mongodb
db.omni_events.distinct(
  "Meta.NearDeployTokenEvent.token_id",
  { "Meta.NearDeployTokenEvent": { $exists: true } }
)
```

## Additional tokens

Using nearblocks we can fetch manually added/migrated tokens by filtering out `migrate_deployed_token` and `add_token` methods, since they don't have an explicit logs and we don't index them

Also, these native tokens should be included:
```txt
eth:0x0000000000000000000000000000000000000000
arb:0x0000000000000000000000000000000000000000
base:0x0000000000000000000000000000000000000000
bnb:0x0000000000000000000000000000000000000000
pol:0x0000000000000000000000000000000000000000
sol:11111111111111111111111111111111
btc:
zcash:
```

And tokens deployed using old factory (`factory.bridge.near`/`factory.sepolia.testnet`) should be added as well (they can be retrieved by calling `get_tokens_accounts` method)
