use near_primitives::runtime::config::ActualRuntimeConfig;
use near_primitives::runtime::config_store::RuntimeConfigStore;
use near_primitives::types::AccountId;
use nearcore::config::{betanet_runtime_config, mainnet_genesis};
use std::convert::TryFrom;

/// Checks that getting configs from RuntimeConfigStore gives backward compatible results with the
/// previous way, i.e. taking runtime config from genesis and modifying it.
#[test]
fn test_mainnet_backwards_compatibility() {
    let genesis = mainnet_genesis();
    let genesis_runtime_config = genesis.config.runtime_config;
    let actual_runtime_config = ActualRuntimeConfig::new(genesis_runtime_config);

    let store = RuntimeConfigStore::new();
    for protocol_version in [29u32, 34u32, 42u32, 50u32].iter() {
        let old_config = actual_runtime_config.for_protocol_version(protocol_version.clone());
        let new_config = store.get_config(protocol_version.clone());
        assert_eq!(old_config, new_config);
    }
}

#[test]
fn test_betanet_backwards_compatibility() {
    let genesis_runtime_config = betanet_runtime_config();
    let actual_runtime_config = ActualRuntimeConfig::new(genesis_runtime_config);

    let store = RuntimeConfigStore::new();
    for protocol_version in [29u32, 34u32, 42u32, 50u32].iter() {
        let mut old_config =
            actual_runtime_config.for_protocol_version(protocol_version.clone()).as_ref().clone();
        old_config.account_creation_config.registrar_account_id =
            AccountId::try_from(String::from("registrar")).unwrap();
        let new_config = store.get_config(protocol_version.clone()).as_ref().clone();

        let str = serde_json::to_string_pretty(&old_config)
            .expect("Failed serializing the runtime config");
        let mut file = File::create("/tmp/old_cfg.json").expect("Failed to create file");
        file.write_all(str.as_bytes());

        assert_eq!(old_config, new_config);
    }
}
