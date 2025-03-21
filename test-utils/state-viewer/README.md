# `state-viewer`

`state-viewer` is a tool that helps you look into the state of the blockchain, which includes:

* apply old blocks with a new version of the code or of the protocol
* generate a genesis file from the current state of the blockchain

## Functions

TODO: Fill out documentation for all available commands

### `apply_range`

Basic example:
```
make release
./target/release/state-viewer --home ~/.near/ apply_range --shard_id=0 --verbose=false --start_index=42376889 --end_index=423770101 --progress=100
```

This command will:
* build a release version of `state-viewer` with link-time optimizations
* open the blockchain state at the location provided by `--home`
* for each block with height between `--start_index` and `--end_index`
  * Run `apply_transactions` function
  * Print individual outcomes if `--verbose` is provided
  * If `--progress` is provided, the tool will print the current statistics to stdout every `--progress` blocks.
* Compute statistics of the gas used and balance burnt for each block and for each receipt within the block

If you want to re-apply all the blocks in the available blockchain then omit both the `--start_index` and `--end_index`
flags. Omitting `--start_index` makes `state-viewer` use blockchain state starting from the genesis. Omitting
`--end_index` makes `state-viewer` use all blocks up to the latest block available in the blockchain.

#### Running for the whole `mainnet` history

As of today you need approximately 2TB of disk space for the whole history of `mainnet`, and the most practical way of
obtaining this whole history is the following:

* Patch https://github.com/near/near-ops/pull/591 to define your own GCP instance in project `rpc-prod`.
* Make sure to change `machine-name` and `role` to something unique.
* Run `terraform init` and `terraform apply` to start an instance. This instance will have a running `neard` systemd 
  service, with `/home/ubuntu/.near` as the home directory.
* SSH using `gcloud compute ssh <machine_name>" --project=rpc-prod`.
* It's recommended to run all the following commands as user `ubuntu`: `sudo su ubuntu`.
* Install tools be able to compile `state-viewer`:
  * Install packages as described here: https://docs.near.org/docs/develop/node/validator/compile-and-run-a-node
  * Install Rust.
  * Clone the git repository.
  * `make release` - note that this compiles not only `state-viewer` but also a few other tools.
* `sudo systemctl stop neard`, because a running node has a LOCK over the database.
* Run `state-viewer` as described above
* Enjoy

#### Checking Predicates

It's hard to know in advance which predicates will be of interest. If you want to check that none of function calls use
more than X gas, feel free to add the check yourself.

### `view_chain`

If called without arguments this command will print the block header of tip of the chain, and chunk extras for that
block.

Flags:

* `--height` gets the block header and chunk extras for a block at a certain height.
* `--block` displays contents of the block itself, such as timestamp, outcome_root, challenges, and many more.
* `--chunk` displays contents of the chunk, such as transactions and receipts.