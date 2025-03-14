use std::sync::Arc;

use integration_tests::node::{Node, ThreadNode};
use near_chain_configs::Genesis;
use near_crypto::{InMemorySigner, KeyType};
use near_logger_utils::init_integration_logger;
use near_network::test_utils::open_port;
use near_primitives::account::AccessKey;
use near_primitives::errors::{InvalidAccessKeyError, InvalidTxError};
use near_primitives::transaction::{
    Action, AddKeyAction, CreateAccountAction, SignedTransaction, TransferAction,
};
use nearcore::config::{GenesisExt, TESTING_INIT_BALANCE, TESTING_INIT_STAKE};
use nearcore::load_test_config;
use testlib::runtime_utils::{alice_account, bob_account};

fn start_node() -> ThreadNode {
    init_integration_logger();
    let genesis = Genesis::test(vec![alice_account(), bob_account()], 1);
    let mut near_config = load_test_config("alice.near", open_port(), genesis);
    near_config.client_config.skip_sync_wait = true;

    let mut node = ThreadNode::new(near_config);
    node.start();
    node
}

#[test]
fn test_check_tx_error_log() {
    let node = start_node();
    let signer =
        Arc::new(InMemorySigner::from_seed(alice_account(), KeyType::ED25519, "alice.near"));
    let block_hash = node.user().get_best_block_hash().unwrap();
    let tx = SignedTransaction::from_actions(
        1,
        bob_account(),
        "test.near".parse().unwrap(),
        &*signer,
        vec![
            Action::CreateAccount(CreateAccountAction {}),
            Action::Transfer(TransferAction { deposit: 1_000 }),
            Action::AddKey(AddKeyAction {
                public_key: signer.public_key.clone(),
                access_key: AccessKey::full_access(),
            }),
        ],
        block_hash,
    );

    let tx_result = node.user().commit_transaction(tx).unwrap_err();
    assert_eq!(
        tx_result,
        InvalidTxError::InvalidAccessKeyError(InvalidAccessKeyError::AccessKeyNotFound {
            account_id: bob_account(),
            public_key: signer.public_key.clone()
        })
        .into()
    );
}

#[test]
fn test_deliver_tx_error_log() {
    let node = start_node();
    let fee_helper = testlib::fees_utils::FeeHelper::new(
        node.genesis().config.runtime_config.transaction_costs.clone(),
        node.genesis().config.min_gas_price,
    );
    let signer =
        Arc::new(InMemorySigner::from_seed(alice_account(), KeyType::ED25519, "alice.near"));
    let block_hash = node.user().get_best_block_hash().unwrap();
    let cost = fee_helper.create_account_transfer_full_key_cost_no_reward();
    let tx = SignedTransaction::from_actions(
        1,
        alice_account(),
        "test.near".parse().unwrap(),
        &*signer,
        vec![
            Action::CreateAccount(CreateAccountAction {}),
            Action::Transfer(TransferAction { deposit: TESTING_INIT_BALANCE + 1 }),
            Action::AddKey(AddKeyAction {
                public_key: signer.public_key.clone(),
                access_key: AccessKey::full_access(),
            }),
        ],
        block_hash,
    );

    let tx_result = node.user().commit_transaction(tx).unwrap_err();
    assert_eq!(
        tx_result,
        InvalidTxError::NotEnoughBalance {
            signer_id: alice_account(),
            balance: TESTING_INIT_BALANCE - TESTING_INIT_STAKE,
            cost: TESTING_INIT_BALANCE + 1 + cost
        }
        .into()
    );
}
