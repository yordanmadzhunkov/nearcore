use serde::{Deserialize, Serialize};

use crate::types::Balance;

/// Data structure for semver version and github tag or commit.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Version {
    pub version: String,
    pub build: String,
}

/// Database version.
pub type DbVersion = u32;

/// Current version of the database.
pub const DB_VERSION: DbVersion = 27;

/// Protocol version type.
pub use near_primitives_core::types::ProtocolVersion;

/// Oldest supported version by this client.
pub const OLDEST_BACKWARD_COMPATIBLE_PROTOCOL_VERSION: ProtocolVersion = 34;

/// Minimum gas price proposed in NEP 92 and the associated protocol version
pub const MIN_GAS_PRICE_NEP_92: Balance = 1_000_000_000;
pub const MIN_PROTOCOL_VERSION_NEP_92: ProtocolVersion = 31;

/// Minimum gas price proposed in NEP 92 (fixed) and the associated protocol version
pub const MIN_GAS_PRICE_NEP_92_FIX: Balance = 100_000_000;
pub const MIN_PROTOCOL_VERSION_NEP_92_FIX: ProtocolVersion = 32;

pub const CORRECT_RANDOM_VALUE_PROTOCOL_VERSION: ProtocolVersion = 33;

/// See [NEP 71](https://github.com/nearprotocol/NEPs/pull/71)
pub const IMPLICIT_ACCOUNT_CREATION_PROTOCOL_VERSION: ProtocolVersion = 35;

/// The protocol version that enables reward on mainnet.
pub const ENABLE_INFLATION_PROTOCOL_VERSION: ProtocolVersion = 36;

/// Fix upgrade to use the latest voted protocol version instead of the current epoch protocol
/// version when there is no new change in protocol version.
pub const UPGRADABILITY_FIX_PROTOCOL_VERSION: ProtocolVersion = 37;

/// Updates the way receipt ID, data ID and random seeds are constructed.
pub const CREATE_HASH_PROTOCOL_VERSION: ProtocolVersion = 38;

/// Fix the storage usage of the delete key action.
pub const DELETE_KEY_STORAGE_USAGE_PROTOCOL_VERSION: ProtocolVersion = 40;

pub const SHARD_CHUNK_HEADER_UPGRADE_VERSION: ProtocolVersion = 41;

/// Updates the way receipt ID is constructed to use current block hash instead of last block hash
pub const CREATE_RECEIPT_ID_SWITCH_TO_CURRENT_BLOCK_VERSION: ProtocolVersion = 42;

pub struct ProtocolVersionRange {
    lower: ProtocolVersion,
    upper: Option<ProtocolVersion>,
}

impl ProtocolVersionRange {
    pub fn new(lower: ProtocolVersion, upper: Option<ProtocolVersion>) -> Self {
        Self { lower, upper }
    }

    pub fn contains(&self, version: ProtocolVersion) -> bool {
        self.lower <= version && self.upper.map_or(true, |upper| version < upper)
    }
}

pub fn is_implicit_account_creation_enabled(protocol_version: ProtocolVersion) -> bool {
    protocol_version >= IMPLICIT_ACCOUNT_CREATION_PROTOCOL_VERSION
}

/// New Protocol features should go here. Features are guarded by their corresponding feature flag.
/// For example, if we have `ProtocolFeature::EVM` and a corresponding feature flag `evm`, it will look
/// like
///
/// #[cfg(feature = "protocol_feature_evm")]
/// EVM code
///
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum ProtocolFeature {
    // stable features
    ForwardChunkParts,
    RectifyInflation,
    AccessKeyNonceRange,
    FixApplyChunks,
    LowerStorageCost,
    DeleteActionRestriction,
    /// Add versions to `Account` data structure
    AccountVersions,
    TransactionSizeLimit,
    /// Fix a bug in `storage_usage` for account caused by #3824
    FixStorageUsage,
    /// Cap maximum gas price to 2,000,000,000 yoctoNEAR
    CapMaxGasPrice,
    CountRefundReceiptsInGasLimit,
    /// Add `ripemd60` and `ecrecover` host function
    MathExtension,
    /// Restore receipts that were previously stuck because of https://github.com/near/nearcore/pull/4228
    RestoreReceiptsAfterFix,

    // nightly features
    #[cfg(feature = "protocol_feature_block_header_v3")]
    BlockHeaderV3,
    #[cfg(feature = "protocol_feature_alt_bn128")]
    AltBn128,
    #[cfg(feature = "protocol_feature_simple_nightshade")]
    SimpleNightshade,
    #[cfg(feature = "protocol_feature_chunk_only_producers")]
    ChunkOnlyProducers,
}

/// Current latest stable version of the protocol.
/// Some features (e. g. FixStorageUsage) require that there is at least one epoch with exactly
/// the corresponding version
#[cfg(not(feature = "nightly_protocol"))]
pub const PROTOCOL_VERSION: ProtocolVersion = 47;

/// Current latest nightly version of the protocol.
#[cfg(feature = "nightly_protocol")]
pub const PROTOCOL_VERSION: ProtocolVersion = 115;

impl ProtocolFeature {
    pub const fn protocol_version(self) -> ProtocolVersion {
        match self {
            // Stable features
            ProtocolFeature::LowerStorageCost => 42,
            ProtocolFeature::DeleteActionRestriction => 43,
            ProtocolFeature::FixApplyChunks => 44,
            ProtocolFeature::ForwardChunkParts => 45,
            ProtocolFeature::RectifyInflation => 45,
            ProtocolFeature::AccessKeyNonceRange => 45,
            ProtocolFeature::AccountVersions => 46,
            ProtocolFeature::TransactionSizeLimit => 46,
            ProtocolFeature::FixStorageUsage => 46,
            ProtocolFeature::CapMaxGasPrice => 46,
            ProtocolFeature::CountRefundReceiptsInGasLimit => 46,
            ProtocolFeature::MathExtension => 46,
            ProtocolFeature::RestoreReceiptsAfterFix => 47,

            // Nightly features
            #[cfg(feature = "protocol_feature_alt_bn128")]
            ProtocolFeature::AltBn128 => 105,
            #[cfg(feature = "protocol_feature_block_header_v3")]
            ProtocolFeature::BlockHeaderV3 => 109,
            #[cfg(feature = "protocol_feature_simple_nightshade")]
            ProtocolFeature::SimpleNightshade => 114,
            #[cfg(feature = "protocol_feature_chunk_only_producers")]
            ProtocolFeature::ChunkOnlyProducers => 115,
        }
    }
}

#[macro_export]
macro_rules! checked_feature {
    ("stable", $feature:ident, $current_protocol_version:expr) => {{
        $crate::version::ProtocolFeature::$feature.protocol_version() <= $current_protocol_version
    }};
    ($feature_name:tt, $feature:ident, $current_protocol_version:expr) => {{
        #[cfg(feature = $feature_name)]
        let is_feature_enabled = $crate::version::ProtocolFeature::$feature.protocol_version()
            <= $current_protocol_version;
        #[cfg(not(feature = $feature_name))]
        let is_feature_enabled = {
            // Workaround unused variable warning
            let _ = $current_protocol_version;

            false
        };
        is_feature_enabled
    }};

    ($feature_name:tt, $feature:ident, $current_protocol_version:expr, $feature_block:block) => {{
        checked_feature!($feature_name, $feature, $current_protocol_version, $feature_block, {})
    }};

    ($feature_name:tt, $feature:ident, $current_protocol_version:expr, $feature_block:block, $non_feature_block:block) => {{
        #[cfg(feature = $feature_name)]
        {
            if checked_feature!($feature_name, $feature, $current_protocol_version) {
                $feature_block
            } else {
                $non_feature_block
            }
        }
        // Workaround unused variable warning
        #[cfg(not(feature = $feature_name))]
        {
            let _ = $current_protocol_version;
            $non_feature_block
        }
    }};
}
