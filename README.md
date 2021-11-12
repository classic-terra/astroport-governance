# Astroport

Uniswap-inspired automated market-maker (AMM) protocol powered by Smart Contracts on the [Terra](https://terra.money) blockchain.

## Contracts

| Name                           | Description                      |
| ------------------------------ | -------------------------------- |
| [`vesting`](contracts/vesting) | ASTRO vesting for team/investors |

## Running this contract

You will need Rust 1.44.1+ with wasm32-unknown-unknown target installed.

For a production-ready (compressed) build, run the following from the repository root:

```
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.12.3
```

The optimized contracts are generated in the artifacts/ directory.
