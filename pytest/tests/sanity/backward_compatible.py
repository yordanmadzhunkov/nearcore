#!/usr/bin/env python3
"""
This script runs node from stable branch and from current branch and makes
sure they are backward compatible.
"""

import sys
import os
import subprocess
import time
import base58
import json

sys.path.append('lib')

import branches
import cluster
from utils import load_test_contract, get_near_tempdir
from transaction import sign_deploy_contract_tx, sign_function_call_tx, sign_payment_tx, sign_create_account_with_full_access_key_and_balance_tx


def main():
    node_root = get_near_tempdir('backward', clean=True)
    branch = branches.latest_rc_branch()
    neard_root, (stable_branch,
                 current_branch) = branches.prepare_ab_test(branch)

    # Setup local network.
    subprocess.call([
        "%sneard-%s" % (neard_root, stable_branch),
        "--home=%s" % node_root, "testnet", "--v", "2", "--prefix", "test"
    ])

    # Run both binaries at the same time.
    config = {
        "local": True,
        'neard_root': neard_root,
        'binary_name': "neard-%s" % stable_branch
    }
    stable_node = cluster.spin_up_node(config, neard_root,
                                       str(node_root / 'test0'), 0, None, None)
    if not os.getenv('NAYDUCK'):
        config["binary_name"] = "neard-%s" % current_branch
    current_node = cluster.spin_up_node(config, neard_root,
                                        str(node_root / 'test1'), 1,
                                        stable_node.node_key.pk,
                                        stable_node.addr())

    # Check it all works.
    BLOCKS = 100
    TIMEOUT = 150
    max_height = -1
    started = time.time()

    # Create account, transfer tokens, deploy contract, invoke function call
    status = stable_node.get_status()
    block_hash = base58.b58decode(
        status['sync_info']['latest_block_hash'].encode('utf-8'))

    new_account_id = 'test_account.test0'
    new_signer_key = cluster.Key(new_account_id, stable_node.signer_key.pk,
                                 stable_node.signer_key.sk)
    create_account_tx = sign_create_account_with_full_access_key_and_balance_tx(
        stable_node.signer_key, new_account_id, new_signer_key, 10**24, 1,
        block_hash)
    res = stable_node.send_tx_and_wait(create_account_tx, timeout=20)
    assert 'error' not in res, res
    assert 'Failure' not in res['result']['status'], res

    transfer_tx = sign_payment_tx(stable_node.signer_key, new_account_id,
                                  10**25, 2, block_hash)
    res = stable_node.send_tx_and_wait(transfer_tx, timeout=20)
    assert 'error' not in res, res

    status = stable_node.get_status()
    block_height = status['sync_info']['latest_block_height']
    nonce = block_height * 1_000_000 - 1

    tx = sign_deploy_contract_tx(new_signer_key, load_test_contract(), nonce,
                                 block_hash)
    res = stable_node.send_tx_and_wait(tx, timeout=20)
    assert 'error' not in res, res

    tx = sign_deploy_contract_tx(stable_node.signer_key, load_test_contract(),
                                 3, block_hash)
    res = stable_node.send_tx_and_wait(tx, timeout=20)
    assert 'error' not in res, res

    tx = sign_function_call_tx(new_signer_key, new_account_id,
                               'write_random_value', [], 10**13, 0, nonce + 1,
                               block_hash)
    res = stable_node.send_tx_and_wait(tx, timeout=20)
    assert 'error' not in res, res
    assert 'Failure' not in res['result']['status'], res

    data = json.dumps([{
        "create": {
            "account_id": "test_account.test0",
            "method_name": "call_promise",
            "arguments": [],
            "amount": "0",
            "gas": 30000000000000,
        },
        "id": 0
    }, {
        "then": {
            "promise_index": 0,
            "account_id": "test0",
            "method_name": "call_promise",
            "arguments": [],
            "amount": "0",
            "gas": 30000000000000,
        },
        "id": 1
    }])

    tx = sign_function_call_tx(stable_node.signer_key,
                               new_account_id, 'call_promise',
                               bytes(data, 'utf-8'), 90000000000000, 0,
                               nonce + 2, block_hash)
    res = stable_node.send_tx_and_wait(tx, timeout=20)

    assert 'error' not in res, res
    assert 'Failure' not in res['result']['status'], res

    while max_height < BLOCKS:
        assert time.time() - started < TIMEOUT
        status = current_node.get_status()
        cur_height = status['sync_info']['latest_block_height']

        if cur_height > max_height:
            max_height = cur_height
        time.sleep(1)


if __name__ == "__main__":
    main()
