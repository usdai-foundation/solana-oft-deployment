# Solana OFT Deployment

This repository contains the OFT program sources deployed on Solana for USDai
and Staked USDai.

## Verification

Install solana-verify 0.5.1:

``` bash
cargo install solana-verify --version 0.5.1 --locked
```

Verify sUSDai Console OFT Program:
``` bash
solana-verify verify-from-repo \
  https://github.com/usdai-foundation/solana-oft-deployment \
  --program-id BQ7nDFGKN4cYqmBkMXFCEzk3zPJhR6bNK9Maf8sQXrQm \
  --library-name console_oft \
  --base-image solanafoundation/solana-verifiable-build:3.1.10 \
  -- \
  --config 'env.OFT_ID="BQ7nDFGKN4cYqmBkMXFCEzk3zPJhR6bNK9Maf8sQXrQm"' \
  --config 'env.HOOK_ID="4EhYYZjhobJpCvp6zWq1xVa8asTimLtADAtxgHh3mhRT"'
```

Verify sUSDai Console Hook Program:
``` bash
solana-verify verify-from-repo \
  https://github.com/usdai-foundation/solana-oft-deployment \
  --program-id 4EhYYZjhobJpCvp6zWq1xVa8asTimLtADAtxgHh3mhRT \
  --library-name console_transfer_hook \
  --base-image solanafoundation/solana-verifiable-build:3.1.10 \
  -- \
  --config 'env.OFT_ID="BQ7nDFGKN4cYqmBkMXFCEzk3zPJhR6bNK9Maf8sQXrQm"' \
  --config 'env.HOOK_ID="4EhYYZjhobJpCvp6zWq1xVa8asTimLtADAtxgHh3mhRT"'
```
