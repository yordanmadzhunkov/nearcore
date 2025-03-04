use crate::types::{PeerMessage, RoutedMessageBody};
use near_metrics::{
    inc_counter_by_opt, inc_counter_opt, try_create_histogram, try_create_int_counter,
    try_create_int_gauge, Histogram, IntCounter, IntGauge,
};
use std::collections::HashMap;
use strum::VariantNames;

use lazy_static::lazy_static;

lazy_static! {
    pub static ref PEER_CONNECTIONS_TOTAL: near_metrics::Result<IntGauge> =
        try_create_int_gauge("near_peer_connections_total", "Number of connected peers");
    pub static ref PEER_DATA_RECEIVED_BYTES: near_metrics::Result<IntCounter> =
        try_create_int_counter("near_peer_data_received_bytes", "Total data received from peers");
    pub static ref PEER_MESSAGE_RECEIVED_TOTAL: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_peer_message_received_total",
            "Number of messages received from peers"
        );
    pub static ref PEER_CLIENT_MESSAGE_RECEIVED_TOTAL: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_peer_client_message_received_total",
            "Number of messages for client received from peers"
        );
    pub static ref PEER_BLOCK_RECEIVED_TOTAL: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_peer_block_received_total",
            "Number of blocks received by peers"
        );
    pub static ref PEER_TRANSACTION_RECEIVED_TOTAL: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_peer_transaction_received_total",
            "Number of transactions received by peers"
        );

    // Routing table metrics
    pub static ref ROUTING_TABLE_RECALCULATIONS: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_routing_table_recalculations_total",
            "Number of times routing table have been recalculated from scratch"
        );


    pub static ref ROUTING_TABLE_RECALCULATION_HISTOGRAM: near_metrics::Result<Histogram> =
        try_create_histogram(
            "near_routing_table_recalculation_seconds",
            "Time spent recalculating routing table"
        );

    pub static ref EDGE_UPDATES: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_edge_updates",
            "Unique edge updates"
        );
    pub static ref EDGE_ACTIVE: near_metrics::Result<IntGauge> =
        try_create_int_gauge(
            "near_edge_active",
            "Total edges active between peers"
        );
    pub static ref PEER_REACHABLE: near_metrics::Result<IntGauge> =
        try_create_int_gauge(
            "near_peer_reachable",
            "Total peers such that there is a path potentially through other peers"
        );
    pub static ref DROP_MESSAGE_UNKNOWN_ACCOUNT: near_metrics::Result<IntCounter> =
        try_create_int_counter(
            "near_drop_message_unknown_account",
            "Total messages dropped because target account is not known"
        );
    pub static ref RECEIVED_INFO_ABOUT_ITSELF: near_metrics::Result<IntCounter> = try_create_int_counter("received_info_about_itself", "Number of times a peer tried to connect to itself");
    pub static ref DROPPED_MESSAGES_COUNT: near_metrics::Result<IntCounter> =
        near_metrics::try_create_int_counter(
            "near_dropped_messages_count",
            "Total count of messages which were dropped, because write buffer was full"
        );
}

#[derive(Clone)]
pub struct NetworkMetrics {
    pub peer_messages: HashMap<String, Option<IntCounter>>,
}

impl NetworkMetrics {
    pub fn new() -> Self {
        let mut peer_messages = HashMap::new();

        let variants = PeerMessage::VARIANTS
            .into_iter()
            .filter(|&name| *name != "Routed")
            .chain(RoutedMessageBody::VARIANTS.into_iter());

        for name in variants {
            let counter_name = NetworkMetrics::peer_message_total_rx(name.as_ref());
            peer_messages.insert(
                counter_name.clone(),
                try_create_int_counter(counter_name.as_ref(), counter_name.as_ref()).ok(),
            );

            let counter_name = NetworkMetrics::peer_message_bytes_rx(name.as_ref());
            peer_messages.insert(
                counter_name.clone(),
                try_create_int_counter(counter_name.as_ref(), counter_name.as_ref()).ok(),
            );

            let counter_name = NetworkMetrics::peer_message_dropped(name.as_ref());
            peer_messages.insert(
                counter_name.clone(),
                try_create_int_counter(counter_name.as_ref(), counter_name.as_ref()).ok(),
            );
        }

        Self { peer_messages }
    }

    pub fn peer_message_total_rx(message_name: &str) -> String {
        format!("near_{}_total", message_name.to_lowercase())
    }

    pub fn peer_message_bytes_rx(message_name: &str) -> String {
        format!("near_{}_bytes", message_name.to_lowercase())
    }

    pub fn peer_message_dropped(message_name: &str) -> String {
        format!("near_{}_dropped", message_name.to_lowercase())
    }

    pub fn inc(&self, message_name: &str) {
        if let Some(counter) = self.peer_messages.get(message_name) {
            inc_counter_opt(counter.as_ref());
        }
    }

    pub fn inc_by(&self, message_name: &str, value: u64) {
        if let Some(counter) = self.peer_messages.get(message_name) {
            inc_counter_by_opt(counter.as_ref(), value);
        }
    }
}
