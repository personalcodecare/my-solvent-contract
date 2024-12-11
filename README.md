## Program Deployment Guide

### Prerequisites

Before proceeding with the deployment, ensure you have the following versions installed:

```sh
anchor --version
# anchor-cli 0.29.0

rustc --version
# rustc 1.75.0 (82e1608df 2023-12-21)

node --version
# v18.15.0

yarn --version
# 1.22.19
```

### Deployment Steps

1. **Initialize Program Keys**
   ```sh
   anchor keys sync
   ```
   This command synchronizes your program's key pairs and updates the necessary configuration files.

2. **Build the Program**
   ```sh
   anchor build
   ```
   This step compiles your Rust program and generates the required artifacts.

3. **Deploy the Program**
   ```sh
   anchor deploy
   ```
   This command deploys your program to the Solana network specified in your configuration.

### Mainnet Deployment Configuration

For production deployment to Solana mainnet, modify your `Anchor.toml` configuration:

```toml
[provider]
cluster = "Mainnet"
wallet = "~/.config/solana/id.json"
```