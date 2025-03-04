# Python-based tests

The directory contains Python-based tests.  The tests are run as part
of nightly testing on NayDuck though they can be run locally as well.

There is no set format of what the tests do but they typical start
a local test cluster using neard binary at `../target/debug/neard`.
There is also some capacity of starting the cluster on remote
machines.


## Running tests

### Running tests locally

To run tests locally first compile a debug build of the nearcore
package, make sure that all required Python packages are installed and
then execute the test file using python.  For example:

    cargo build
    cd pytest
    python -m pip install -U requirements.txt
    python tests/sanity/one_val.py

After the test finishes, log files and other result data from running
each node will be located in a `~/.near/test#_finished` directory
(where `#` is index of the node starting with zero).

Note that running the tests using `pytest` command is not supported
and won’t work reliably.

Furthermore, running multiple tests at once is not supported either
because tests often use hard-coded paths (e.g. `~/.node/test#` for
node home directories) and port numbers

### Ruining tests on NayDuck

As mentioned, the tests are normally run nightly on NayDuck.  To
schedule a run on NayDuck manual the `../scripts/nayduck.py` script is
used.  The `../nightly/README.md` file describes this is more detail.

### Running pytest remotely

The test library has code for executing tests while running the nodes
on remote Google Cloud machines.  Presumably that code worked in the
past but I, mina86, haven’t tried it and am a bit sceptical as to
whether it is still functional.  Regardless, for anyone who wants to
try it out, the instructions are as follows:

Prerequisites:

1. Same as local pytest
2. gcloud cli in PATH

Steps:

1. Choose or upload a near binary here: https://console.cloud.google.com/storage/browser/nearprotocol_nearcore_release?project=near-core
2. Fill the binary filename in remote.json.  Modify zones as needed,
   they’ll be used in round-robin manner.
3. `NEAR_PYTEST_CONFIG=remote.json python tests/...`
4. Run `python tests/delete_remote_nodes.py` to make sure the remote
   nodes are shut down properly (especially if tests failed early).


## Creating new tests

To add a test simply create a Python script inside of the `tests`
directory and add it to a test set file in `../nightly` directory.
See `../nightly/README.md` file for detailed documentation of the test
set files.  Note that if you add a test file but don’t include it in
nightly test set the pull request check will fail.

Even though this directory is called `pytest`, the tests need to work
when executed via `python`.  This means that they need to execute the
tests when run as the main module rather than just defining the tests
function.  To make that happen it’s best to define `test_<foo>`
functions with test bodies and than execute all those functions in
a code fragment guarded by `if __name__ == '__main__'` condition.

If the test operates on the nodes running in a cluster, it will very
likely want to make use of `start_cluster` function defined in the
`lib/cluster.py` module.

For example, a simple test for checking implementation of
`max_gas_burnt_view` could be located in
`tests/sanity/rpc_max_gas_burnt.py` and look as follows:

    """Test max_gas_burnt_view client configuration.

    Spins up two nodes with different max_gas_burnt_view client
    configuration, deploys a smart contract and finally calls a view
    function against both nodes expecting the one with low
    max_gas_burnt_view limit to fail.
    """

    import sys
    import base58
    import base64
    import json

    sys.path.append('lib')
    from cluster import start_cluster
    from utils import load_binary_file
    import transaction


    def test_max_gas_burnt_view():
        nodes = start_cluster(2, 0, 1,
                              config=None,
                              genesis_config_changes=[],
                              client_config_changes={
                                  1: {'max_gas_burnt_view': int(5e10)}
                              })

        contract_key = nodes[0].signer_key
        contract = load_binary_file(
            '../runtime/near-test-contracts/res/test_contract_rs.wasm')

        # Deploy the fib smart contract
        status = nodes[0].get_status()
        latest_block_hash = status['sync_info']['latest_block_hash']
        deploy_contract_tx = transaction.sign_deploy_contract_tx(
            contract_key, contract, 10,
            base58.b58decode(latest_block_hash.encode('utf8')))
        deploy_contract_response = (
            nodes[0].send_tx_and_wait(deploy_contract_tx, 10))

        def call_fib(node, n):
            args = base64.b64encode(bytes([n])).decode('ascii')
            return node.call_function(
                contract_key.account_id, 'fibonacci', args,
                timeout=10
            ).get('result')

        # Call view function of the smart contract via the first
        # node.  This should succeed.
        result = call_fib(nodes[0], 25)
        assert 'result' in result and 'error' not in result, (
            'Expected "result" and no "error" in response, got: {}'
                .format(result))

        # Same but against the second node.  This should fail.
        result = call_fib(nodes[1], 25)
        assert 'result' not in result and 'error' in result, (
            'Expected "error" and no "result" in response, got: {}'
                .format(result))
        error = result['error']
        assert 'HostError(GasLimitExceeded)' in error, (
            'Expected error due to GasLimitExceeded but got: {}'.format(error))


    if __name__ == '__main__':
        test_max_gas_burnt_view()
