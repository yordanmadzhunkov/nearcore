use std::time::Duration;

use actix::clock::sleep;
use actix::{Actor, System};
use borsh::BorshSerialize;
use futures::future::join_all;
use futures::{future, FutureExt, TryFutureExt};

use integration_tests::genesis_helpers::genesis_block;
use near_actix_test_utils::spawn_interruptible;
use near_client::{GetBlock, GetExecutionOutcome, GetValidatorInfo};
use near_crypto::{InMemorySigner, KeyType};
use near_jsonrpc::client::new_client;
use near_logger_utils::init_integration_logger;
use near_network::test_utils::WaitOrTimeout;
use near_primitives::hash::{hash, CryptoHash};
use near_primitives::merkle::{compute_root_from_path_and_item, verify_path};
use near_primitives::serialize::{from_base64, to_base64};
use near_primitives::transaction::{PartialExecutionStatus, SignedTransaction};
use near_primitives::types::{
    BlockId, BlockReference, EpochId, EpochReference, Finality, TransactionOrReceiptId,
};
use near_primitives::views::{ExecutionOutcomeView, ExecutionStatusView};

use crate::node_cluster::NodeCluster;

macro_rules! panic_on_rpc_error {
    ($e:expr) => {
        if !serde_json::to_string(&$e.data.clone().unwrap_or_default())
            .unwrap()
            .contains("IsSyncing")
        {
            panic!("{:?}", $e)
        }
    };
}

#[test]
fn test_get_validator_info_rpc() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("validator_info{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|_, rpc_addrs, clients| async move {
        WaitOrTimeout::new(
            Box::new(move |_ctx| {
                let rpc_addrs_copy = rpc_addrs.clone();
                let view_client = clients[0].1.clone();
                spawn_interruptible(async move {
                    let block_view = view_client.send(GetBlock::latest()).await.unwrap().unwrap();
                    if block_view.header.height > 1 {
                        let client = new_client(&format!("http://{}", rpc_addrs_copy[0]));
                        let block_hash = block_view.header.hash;
                        let invalid_res = client.validators(Some(BlockId::Hash(block_hash))).await;
                        assert!(invalid_res.is_err());
                        let res = client.validators(None).await.unwrap();

                        assert_eq!(res.current_validators.len(), 1);
                        assert!(res
                            .current_validators
                            .iter()
                            .any(|r| r.account_id.as_ref() == "near.0"));
                        System::current().stop();
                    }
                });
            }),
            100,
            40000,
        )
        .start();
    });
}

fn outcome_view_to_hashes(outcome: &ExecutionOutcomeView) -> Vec<CryptoHash> {
    let status = match &outcome.status {
        ExecutionStatusView::Unknown => PartialExecutionStatus::Unknown,
        ExecutionStatusView::SuccessValue(s) => {
            PartialExecutionStatus::SuccessValue(from_base64(s).unwrap())
        }
        ExecutionStatusView::Failure(_) => PartialExecutionStatus::Failure,
        ExecutionStatusView::SuccessReceiptId(id) => PartialExecutionStatus::SuccessReceiptId(*id),
    };
    let mut result = vec![hash(
        &(
            outcome.receipt_ids.clone(),
            outcome.gas_burnt,
            outcome.tokens_burnt,
            outcome.executor_id.clone(),
            status,
        )
            .try_to_vec()
            .expect("Failed to serialize"),
    )];
    for log in outcome.logs.iter() {
        result.push(hash(log.as_bytes()));
    }
    result
}

fn test_get_execution_outcome(is_tx_successful: bool) {
    init_integration_logger();

    let cluster = NodeCluster::new(2, |index| format!("tx_propagation{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(1)
        .set_epoch_length(1000)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        let genesis_hash = *genesis_block(&genesis).hash();
        let signer =
            InMemorySigner::from_seed("near.0".parse().unwrap(), KeyType::ED25519, "near.0");
        let transaction = if is_tx_successful {
            SignedTransaction::send_money(
                1,
                "near.0".parse().unwrap(),
                "near.1".parse().unwrap(),
                &signer,
                10000,
                genesis_hash,
            )
        } else {
            SignedTransaction::create_account(
                1,
                "near.0".parse().unwrap(),
                "near.1".parse().unwrap(),
                10,
                signer.public_key.clone(),
                &signer,
                genesis_hash,
            )
        };

        WaitOrTimeout::new(
            Box::new(move |_ctx| {
                let client = new_client(&format!("http://{}", rpc_addrs[0]));
                let bytes = transaction.try_to_vec().unwrap();
                let view_client1 = view_client.clone();
                spawn_interruptible(client.broadcast_tx_commit(to_base64(&bytes)).then(
                    move |res| {
                        let final_transaction_outcome = match res {
                            Ok(outcome) => outcome,
                            Err(_) => return future::ready(()),
                        };
                        spawn_interruptible(sleep(Duration::from_secs(1)).then(move |_| {
                            let mut futures = vec![];
                            for id in vec![TransactionOrReceiptId::Transaction {
                                transaction_hash: final_transaction_outcome.transaction_outcome.id,
                                sender_id: "near.0".parse().unwrap(),
                            }]
                            .into_iter()
                            .chain(
                                final_transaction_outcome.receipts_outcome.into_iter().map(|r| {
                                    TransactionOrReceiptId::Receipt {
                                        receipt_id: r.id,
                                        receiver_id: "near.1".parse().unwrap(),
                                    }
                                }),
                            ) {
                                let view_client2 = view_client1.clone();
                                let fut = view_client1.send(GetExecutionOutcome { id }).then(
                                    move |res| {
                                        let execution_outcome_response = res.unwrap().unwrap();
                                        view_client2
                                            .send(GetBlock(BlockReference::BlockId(BlockId::Hash(
                                                execution_outcome_response.outcome_proof.block_hash,
                                            ))))
                                            .then(move |res| {
                                                let res = res.unwrap().unwrap();
                                                let mut outcome_with_id_to_hash = vec![
                                                    execution_outcome_response.outcome_proof.id,
                                                ];
                                                outcome_with_id_to_hash.extend(
                                                    outcome_view_to_hashes(
                                                        &execution_outcome_response
                                                            .outcome_proof
                                                            .outcome,
                                                    ),
                                                );
                                                let chunk_outcome_root =
                                                    compute_root_from_path_and_item(
                                                        &execution_outcome_response
                                                            .outcome_proof
                                                            .proof,
                                                        &outcome_with_id_to_hash,
                                                    );
                                                assert!(verify_path(
                                                    res.header.outcome_root,
                                                    &execution_outcome_response.outcome_root_proof,
                                                    &chunk_outcome_root
                                                ));
                                                future::ready(())
                                            })
                                    },
                                );
                                futures.push(fut);
                            }
                            spawn_interruptible(join_all(futures).then(|_| {
                                System::current().stop();
                                future::ready(())
                            }));
                            future::ready(())
                        }));

                        future::ready(())
                    },
                ));
            }),
            100,
            40000,
        )
        .start();
    });
}

#[test]
fn test_get_execution_outcome_tx_success() {
    test_get_execution_outcome(true);
}

#[test]
fn test_get_execution_outcome_tx_failure() {
    test_get_execution_outcome(false);
}

#[test]
fn test_protocol_config_rpc() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("protocol_config{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, _| async move {
        let client = new_client(&format!("http://{}", rpc_addrs[0]));
        let config_response = client
            .EXPERIMENTAL_protocol_config(
                near_jsonrpc_primitives::types::config::RpcProtocolConfigRequest {
                    block_reference: near_primitives::types::BlockReference::Finality(
                        Finality::None,
                    ),
                },
            )
            .await
            .unwrap();
        assert_ne!(
            config_response.config_view.runtime_config.storage_amount_per_byte,
            genesis.config.runtime_config.storage_amount_per_byte
        );
        assert_eq!(
            config_response.config_view.runtime_config.storage_amount_per_byte,
            10u128.pow(19)
        );
        System::current().stop();
    });
}

#[test]
fn test_query_rpc_account_view_must_succeed() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("protocol_config{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|_, rpc_addrs, _| async move {
        let client = new_client(&format!("http://{}", rpc_addrs[0]));
        let query_response = client
            .query(near_jsonrpc_primitives::types::query::RpcQueryRequest {
                block_reference: near_primitives::types::BlockReference::Finality(Finality::Final),
                request: near_primitives::views::QueryRequest::ViewAccount {
                    account_id: "near.0".parse().unwrap(),
                },
            })
            .await
            .unwrap();
        let account =
            if let near_jsonrpc_primitives::types::query::QueryResponseKind::ViewAccount(account) =
                query_response.kind
            {
                account
            } else {
                panic!(
                    "expected a account view result, but received something else: {:?}",
                    query_response.kind
                );
            };
        assert!(matches!(account, near_primitives::views::AccountView { .. }));
        System::current().stop();
    });
}

#[test]
fn test_query_rpc_account_view_account_doesnt_exist_must_return_error() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("protocol_config{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|_, rpc_addrs, _| async move {
        let client = new_client(&format!("http://{}", rpc_addrs[0]));
        let query_response = client
            .query(near_jsonrpc_primitives::types::query::RpcQueryRequest {
                block_reference: near_primitives::types::BlockReference::Finality(Finality::Final),
                request: near_primitives::views::QueryRequest::ViewAccount {
                    account_id: "accountdoesntexist.0".parse().unwrap(),
                },
            })
            .await;

        let error_message = match query_response {
            Ok(result) => panic!("expected error but received Ok: {:?}", result.kind),
            Err(err) => err.data.unwrap(),
        };

        assert!(
            error_message
                .to_string()
                .contains("account accountdoesntexist.0 does not exist while viewing"),
            "{}",
            error_message
        );

        System::current().stop();
    });
}

#[test]
fn test_tx_not_enough_balance_must_return_error() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("tx_not_enough_balance{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(2)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        let genesis_hash = *genesis_block(&genesis).hash();
        let signer =
            InMemorySigner::from_seed("near.0".parse().unwrap(), KeyType::ED25519, "near.0");
        let transaction = SignedTransaction::send_money(
            1,
            "near.0".parse().unwrap(),
            "near.1".parse().unwrap(),
            &signer,
            1100000000000000000000000000000000,
            genesis_hash,
        );

        let client = new_client(&format!("http://{}", rpc_addrs[0]));
        let bytes = transaction.try_to_vec().unwrap();

        spawn_interruptible(async move {
            loop {
                let res = view_client.send(GetBlock::latest()).await;
                if let Ok(Ok(block)) = res {
                    if block.header.height > 10 {
                        let _ = client
                            .broadcast_tx_commit(to_base64(&bytes))
                            .map_err(|err| {
                                assert_eq!(
                                    err.data.unwrap(),
                                    serde_json::json!({"TxExecutionError": {
                                        "InvalidTxError": {
                                            "NotEnoughBalance": {
                                                "signer_id": "near.0",
                                                "balance": "950000000000000000000000000000000", // If something changes in setup just update this value
                                                "cost": "1100000000000453060601875000000000",
                                            }
                                        }
                                    }})
                                );
                                System::current().stop();
                            })
                            .map_ok(|_| panic!("Transaction must not succeed"))
                            .await;
                        break;
                    }
                }
                sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    });
}

#[test]
fn test_send_tx_sync_returns_transaction_hash() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("tx_not_enough_balance{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        let genesis_hash = *genesis_block(&genesis).hash();
        let signer =
            InMemorySigner::from_seed("near.0".parse().unwrap(), KeyType::ED25519, "near.0");
        let transaction = SignedTransaction::send_money(
            1,
            "near.0".parse().unwrap(),
            "near.0".parse().unwrap(),
            &signer,
            10000,
            genesis_hash,
        );

        let client = new_client(&format!("http://{}", rpc_addrs[0]));
        let tx_hash = transaction.get_hash();
        let bytes = transaction.try_to_vec().unwrap();

        spawn_interruptible(async move {
            loop {
                let res = view_client.send(GetBlock::latest()).await;
                if let Ok(Ok(block)) = res {
                    if block.header.height > 10 {
                        let response = client
                            .EXPERIMENTAL_broadcast_tx_sync(to_base64(&bytes))
                            .map_err(|err| panic_on_rpc_error!(err))
                            .await
                            .unwrap();
                        assert_eq!(response["transaction_hash"], tx_hash.to_string());
                        System::current().stop();
                        break;
                    }
                }
                sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    });
}

#[test]
fn test_send_tx_sync_to_lightclient_must_be_routed() {
    init_integration_logger();

    let cluster = NodeCluster::new(2, |index| format!("tx_routed{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(1)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        let genesis_hash = *genesis_block(&genesis).hash();
        let signer =
            InMemorySigner::from_seed("near.1".parse().unwrap(), KeyType::ED25519, "near.1");
        let transaction = SignedTransaction::send_money(
            1,
            "near.1".parse().unwrap(),
            "near.1".parse().unwrap(),
            &signer,
            10000,
            genesis_hash,
        );

        let client = new_client(&format!("http://{}", rpc_addrs[1]));
        let tx_hash = transaction.get_hash();
        let bytes = transaction.try_to_vec().unwrap();

        spawn_interruptible(async move {
            loop {
                let res = view_client.send(GetBlock::latest()).await;
                if let Ok(Ok(block)) = res {
                    if block.header.height > 10 {
                        let _ = client
                            .EXPERIMENTAL_broadcast_tx_sync(to_base64(&bytes))
                            .map_err(|err| {
                                assert_eq!(
                                    err.data.unwrap(),
                                    serde_json::json!(format!(
                                        "Transaction with hash {} was routed",
                                        tx_hash
                                    ))
                                );
                                System::current().stop();
                            })
                            .map_ok(|_| panic!("Transaction must not succeed"))
                            .await;
                        break;
                    }
                }
                sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    });
}

#[test]
fn test_check_unknown_tx_must_return_error() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("tx_unknown{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        let genesis_hash = *genesis_block(&genesis).hash();
        let signer =
            InMemorySigner::from_seed("near.0".parse().unwrap(), KeyType::ED25519, "near.0");
        let transaction = SignedTransaction::send_money(
            1,
            "near.0".parse().unwrap(),
            "near.0".parse().unwrap(),
            &signer,
            10000,
            genesis_hash,
        );

        let client = new_client(&format!("http://{}", rpc_addrs[0]));
        let tx_hash = transaction.get_hash();
        let bytes = transaction.try_to_vec().unwrap();

        spawn_interruptible(async move {
            loop {
                let res = view_client.send(GetBlock::latest()).await;
                if let Ok(Ok(block)) = res {
                    if block.header.height > 10 {
                        let _ = client
                            .EXPERIMENTAL_tx_status(to_base64(&bytes))
                            .map_err(|err| {
                                assert_eq!(
                                    err.data.unwrap(),
                                    serde_json::json!(format!(
                                        "Transaction {} doesn't exist",
                                        tx_hash
                                    ))
                                );
                                System::current().stop();
                            })
                            .map_ok(|_| panic!("Transaction must be unknown"))
                            .await;
                        break;
                    }
                }
                sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    });
}

#[test]
fn test_check_tx_on_lightclient_must_return_does_not_track_shard() {
    init_integration_logger();

    let cluster = NodeCluster::new(2, |index| format!("tx_does_not_track_shard{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(1)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|genesis, rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        let genesis_hash = *genesis_block(&genesis).hash();
        let signer = InMemorySigner::from_seed("near.1".parse().unwrap(), KeyType::ED25519, "near.1");
        let transaction = SignedTransaction::send_money(
            1,
            "near.1".parse().unwrap(),
            "near.1".parse().unwrap(),
            &signer,
            10000,
            genesis_hash,
        );

        let client = new_client(&format!("http://{}", rpc_addrs[1]));
        let bytes = transaction.try_to_vec().unwrap();

        spawn_interruptible(async move {
            loop {
                let res = view_client.send(GetBlock::latest()).await;
                if let Ok(Ok(block)) = res {
                    if block.header.height > 10 {
                        let _ = client
                            .EXPERIMENTAL_check_tx(to_base64(&bytes))
                            .map_err(|err| {
                                assert_eq!(
                                    err.data.unwrap(),
                                    serde_json::json!("Node doesn't track this shard. Cannot determine whether the transaction is valid")
                                );
                                System::current().stop();
                            })
                            .map_ok(|_| panic!("Must not track shard"))
                            .await;
                        break;
                    }
                }
                sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    });
}

#[test]
fn test_validators_by_epoch_id_current_epoch_not_fails() {
    init_integration_logger();

    let cluster = NodeCluster::new(1, |index| format!("validators_epoch_id{}", index))
        .set_num_shards(1)
        .set_num_validator_seats(1)
        .set_num_lightclients(0)
        .set_epoch_length(10)
        .set_genesis_height(0);

    cluster.exec_until_stop(|_genesis, _rpc_addrs, clients| async move {
        let view_client = clients[0].1.clone();

        spawn_interruptible(async move {
            let final_block = loop {
                let res = view_client.send(GetBlock::latest()).await;
                if let Ok(Ok(block)) = res {
                    if block.header.height > 1 {
                        break block;
                    }
                }
            };

            let res = view_client
                .send(GetValidatorInfo {
                    epoch_reference: EpochReference::EpochId(EpochId(final_block.header.epoch_id)),
                })
                .await;

            match res {
                Ok(Ok(validators)) => {
                    assert_eq!(validators.current_validators.len(), 1);
                    System::current().stop();
                }
                err => panic!("Validators list by EpochId must succeed: {:?}", err),
            }
        });
    });
}
