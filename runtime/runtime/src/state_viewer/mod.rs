use crate::{actions::execute_function_call, ext::RuntimeExt};
use log::debug;
use near_crypto::{KeyType, PublicKey};
use near_primitives::{
    account::{AccessKey, Account},
    borsh::BorshDeserialize,
    config::VMLimitConfig,
    contract::ContractCode,
    hash::CryptoHash,
    receipt::ActionReceipt,
    runtime::{
        apply_state::ApplyState,
        config::RuntimeConfig,
        migration_data::{MigrationData, MigrationFlags},
    },
    serialize::to_base64,
    transaction::FunctionCallAction,
    trie_key::trie_key_parsers,
    types::{AccountId, EpochInfoProvider, Gas},
    views::{StateItem, ViewApplyState, ViewStateResult},
};
use near_store::{get_access_key, get_account, get_code, TrieUpdate};
use near_vm_logic::{ReturnData, ViewConfig};
use std::{str, sync::Arc, time::Instant};

pub mod errors;

pub struct TrieViewer {
    /// Upper bound of the byte size of contract state that is still viewable. None is no limit
    state_size_limit: Option<u64>,
    /// Gas limit used when when handling call_function queries.
    max_gas_burnt_view: Gas,
}

impl Default for TrieViewer {
    fn default() -> Self {
        Self { state_size_limit: None, max_gas_burnt_view: VMLimitConfig::default().max_gas_burnt }
    }
}

impl TrieViewer {
    pub fn new(state_size_limit: Option<u64>, max_gas_burnt_view: Option<Gas>) -> Self {
        let max_gas_burnt_view =
            max_gas_burnt_view.unwrap_or_else(|| TrieViewer::default().max_gas_burnt_view);
        Self { state_size_limit, max_gas_burnt_view }
    }

    pub fn view_account(
        &self,
        state_update: &TrieUpdate,
        account_id: &AccountId,
    ) -> Result<Account, errors::ViewAccountError> {
        get_account(state_update, &account_id)?.ok_or_else(|| {
            errors::ViewAccountError::AccountDoesNotExist {
                requested_account_id: account_id.clone(),
            }
        })
    }

    pub fn view_contract_code(
        &self,
        state_update: &TrieUpdate,
        account_id: &AccountId,
    ) -> Result<ContractCode, errors::ViewContractCodeError> {
        let account = self.view_account(state_update, account_id)?;
        get_code(state_update, account_id, Some(account.code_hash()))?.ok_or_else(|| {
            errors::ViewContractCodeError::NoContractCode {
                contract_account_id: account_id.clone(),
            }
        })
    }

    pub fn view_access_key(
        &self,
        state_update: &TrieUpdate,
        account_id: &AccountId,
        public_key: &PublicKey,
    ) -> Result<AccessKey, errors::ViewAccessKeyError> {
        get_access_key(state_update, account_id, public_key)?.ok_or_else(|| {
            errors::ViewAccessKeyError::AccessKeyDoesNotExist { public_key: public_key.clone() }
        })
    }

    pub fn view_access_keys(
        &self,
        state_update: &TrieUpdate,
        account_id: &AccountId,
    ) -> Result<Vec<(PublicKey, AccessKey)>, errors::ViewAccessKeyError> {
        let prefix = trie_key_parsers::get_raw_prefix_for_access_keys(account_id);
        let raw_prefix: &[u8] = prefix.as_ref();
        let access_keys =
            state_update
                .iter(&prefix)?
                .map(|key| {
                    let key = key?;
                    let public_key = &key[raw_prefix.len()..];
                    let access_key = near_store::get_access_key_raw(&state_update, &key)?
                        .ok_or_else(|| errors::ViewAccessKeyError::InternalError {
                            error_message: "Unexpected missing key from iterator".to_string(),
                        })?;
                    PublicKey::try_from_slice(public_key)
                        .map_err(|_| errors::ViewAccessKeyError::InternalError {
                            error_message: format!(
                                "Unexpected invalid public key {:?} received from store",
                                public_key
                            ),
                        })
                        .map(|key| (key, access_key))
                })
                .collect::<Result<Vec<_>, errors::ViewAccessKeyError>>();
        access_keys
    }

    pub fn view_state(
        &self,
        state_update: &TrieUpdate,
        account_id: &AccountId,
        prefix: &[u8],
    ) -> Result<ViewStateResult, errors::ViewStateError> {
        match get_account(state_update, account_id)? {
            Some(account) => {
                let code_len = get_code(state_update, account_id, Some(account.code_hash()))?
                    .map(|c| c.code().len() as u64)
                    .unwrap_or_default();
                if let Some(limit) = self.state_size_limit {
                    if account.storage_usage().saturating_sub(code_len) > limit {
                        return Err(errors::ViewStateError::AccountStateTooLarge {
                            requested_account_id: account_id.clone(),
                        });
                    }
                }
            }
            None => {
                return Err(errors::ViewStateError::AccountDoesNotExist {
                    requested_account_id: account_id.clone(),
                })
            }
        };

        let mut values = vec![];
        let query = trie_key_parsers::get_raw_prefix_for_contract_data(account_id, prefix);
        let acc_sep_len = query.len() - prefix.len();
        let mut iter = state_update.trie.iter(&state_update.get_root())?;
        iter.seek(&query)?;
        for item in iter {
            let (key, value) = item?;
            if !key.starts_with(&query.as_ref()) {
                break;
            }
            values.push(StateItem {
                key: to_base64(&key[acc_sep_len..]),
                value: to_base64(&value),
                proof: vec![],
            });
        }
        // TODO(2076): Add proofs for the storage items.
        Ok(ViewStateResult { values, proof: vec![] })
    }

    pub fn call_function(
        &self,
        mut state_update: TrieUpdate,
        view_state: ViewApplyState,
        contract_id: &AccountId,
        method_name: &str,
        args: &[u8],
        logs: &mut Vec<String>,
        epoch_info_provider: &dyn EpochInfoProvider,
    ) -> Result<Vec<u8>, errors::CallFunctionError> {
        let now = Instant::now();
        let root = state_update.get_root();
        let mut account = get_account(&state_update, contract_id)?.ok_or_else(|| {
            errors::CallFunctionError::AccountDoesNotExist {
                requested_account_id: contract_id.clone(),
            }
        })?;
        // TODO(#1015): Add ability to pass public key and originator_id
        let originator_id = contract_id;
        let public_key = PublicKey::empty(KeyType::ED25519);
        let empty_hash = CryptoHash::default();
        let mut runtime_ext = RuntimeExt::new(
            &mut state_update,
            contract_id,
            originator_id,
            &public_key,
            0,
            &empty_hash,
            &view_state.epoch_id,
            &view_state.prev_block_hash,
            &view_state.block_hash,
            epoch_info_provider,
            view_state.current_protocol_version,
        );
        let config = Arc::new(RuntimeConfig::default());
        let apply_state = ApplyState {
            block_index: view_state.block_height,
            // Used for legacy reasons
            prev_block_hash: view_state.prev_block_hash,
            block_hash: view_state.block_hash,
            epoch_id: view_state.epoch_id.clone(),
            epoch_height: view_state.epoch_height,
            gas_price: 0,
            block_timestamp: view_state.block_timestamp,
            gas_limit: None,
            random_seed: root,
            current_protocol_version: view_state.current_protocol_version,
            config: config.clone(),
            cache: view_state.cache,
            is_new_chunk: false,
            migration_data: Arc::new(MigrationData::default()),
            migration_flags: MigrationFlags::default(),
        };
        let action_receipt = ActionReceipt {
            signer_id: originator_id.clone(),
            signer_public_key: public_key.clone(),
            gas_price: 0,
            output_data_receivers: vec![],
            input_data_ids: vec![],
            actions: vec![],
        };
        let function_call = FunctionCallAction {
            method_name: method_name.to_string(),
            args: args.to_vec(),
            gas: self.max_gas_burnt_view,
            deposit: 0,
        };
        let (outcome, err) = execute_function_call(
            &apply_state,
            &mut runtime_ext,
            &mut account,
            &originator_id,
            &action_receipt,
            &[],
            &function_call,
            &empty_hash,
            &config,
            true,
            Some(ViewConfig { max_gas_burnt: self.max_gas_burnt_view }),
        );
        let elapsed = now.elapsed();
        let time_ms =
            (elapsed.as_secs() as f64 / 1_000.0) + f64::from(elapsed.subsec_nanos()) / 1_000_000.0;
        let time_str = format!("{:.*}ms", 2, time_ms);

        if let Some(err) = err {
            if let Some(outcome) = outcome {
                logs.extend(outcome.logs);
            }
            let message = format!("wasm execution failed with error: {:?}", err);
            debug!(target: "runtime", "(exec time {}) {}", time_str, message);
            Err(errors::CallFunctionError::VMError { error_message: message })
        } else {
            let outcome = outcome.unwrap();
            debug!(target: "runtime", "(exec time {}) result of execution: {:?}", time_str, outcome);
            logs.extend(outcome.logs);
            let result = match outcome.return_data {
                ReturnData::Value(buf) => buf,
                ReturnData::ReceiptIndex(_) | ReturnData::None => vec![],
            };
            Ok(result)
        }
    }
}
