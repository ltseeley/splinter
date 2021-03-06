// Copyright 2018-2020 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::{fmt, path::Path, time::SystemTime};

use protobuf::Message;
use sawtooth::store::lmdb::LmdbOrderedStore;
use sawtooth::store::receipt_store::TransactionReceiptStore;
use sawtooth_sabre::handler::SabreTransactionHandler;
use sawtooth_sabre::{ADMINISTRATORS_SETTING_ADDRESS, ADMINISTRATORS_SETTING_KEY};
use transact::context::manager::sync::ContextManager;
use transact::database::{
    lmdb::{LmdbContext, LmdbDatabase},
    Database,
};
use transact::sawtooth::SawtoothToTransactHandlerAdapter;
use transact::scheduler::{
    serial::SerialScheduler, BatchExecutionResult, InvalidTransactionResult, Scheduler,
    TransactionExecutionResult,
};
use transact::state::{
    merkle::{MerkleRadixTree, MerkleState, INDEXES},
    StateChange as TransactStateChange, Write,
};
use transact::{
    execution::{adapter::static_adapter::StaticExecutionAdapter, executor::Executor},
    protocol::{batch::BatchPair, receipt::TransactionReceipt},
};

#[cfg(feature = "events")]
use crate::events::{ParseBytes, ParseError};
use crate::hex;
use crate::protos::scabbard::{Setting, Setting_Entry};

use super::error::{ScabbardStateError, StateSubscriberError};

const EXECUTION_TIMEOUT: u64 = 300; // five minutes

const CURRENT_STATE_ROOT_INDEX: &str = "current_state_root";

const ITER_CACHE_SIZE: usize = 64;

pub struct ScabbardState {
    db: Box<dyn Database>,
    context_manager: ContextManager,
    executor: Executor,
    current_state_root: String,
    transaction_receipt_store: Arc<RwLock<TransactionReceiptStore>>,
    pending_changes: Option<(String, Vec<TransactionReceipt>)>,
    event_subscribers: Vec<Box<dyn StateSubscriber>>,
    batch_history: BatchHistory,
}

impl ScabbardState {
    pub fn new(
        state_db_path: &Path,
        state_db_size: usize,
        receipt_db_path: &Path,
        receipt_db_size: usize,
        admin_keys: Vec<String>,
    ) -> Result<Self, ScabbardStateError> {
        // Initialize the database
        let mut indexes = INDEXES.to_vec();
        indexes.push(CURRENT_STATE_ROOT_INDEX);
        let db = Box::new(LmdbDatabase::new(
            LmdbContext::new(state_db_path, indexes.len(), Some(state_db_size))?,
            &indexes,
        )?);

        let current_state_root = if let Some(current_state_root) =
            Self::read_current_state_root(&*db)?
        {
            debug!("Restoring scabbard state on root {}", current_state_root);
            current_state_root
        } else {
            // Set initial state (admin keys)
            let mut admin_keys_entry = Setting_Entry::new();
            admin_keys_entry.set_key(ADMINISTRATORS_SETTING_KEY.into());
            admin_keys_entry.set_value(admin_keys.join(","));
            let mut admin_keys_setting = Setting::new();
            admin_keys_setting.set_entries(vec![admin_keys_entry].into());
            let admin_keys_setting_bytes = admin_keys_setting.write_to_bytes().map_err(|err| {
                ScabbardStateError(format!(
                    "failed to write admin keys setting to bytes: {}",
                    err
                ))
            })?;
            let admin_keys_state_change = TransactStateChange::Set {
                key: ADMINISTRATORS_SETTING_ADDRESS.into(),
                value: admin_keys_setting_bytes,
            };

            let initial_state_root = MerkleRadixTree::new(db.clone_box(), None)?.get_merkle_root();
            MerkleState::new(db.clone()).commit(
                &initial_state_root,
                vec![admin_keys_state_change].as_slice(),
            )?
        };

        // Initialize transact
        let context_manager = ContextManager::new(Box::new(MerkleState::new(db.clone())));
        let mut executor = Executor::new(vec![Box::new(StaticExecutionAdapter::new_adapter(
            vec![Box::new(SawtoothToTransactHandlerAdapter::new(
                SabreTransactionHandler::new(),
            ))],
            context_manager.clone(),
        )?)]);
        executor
            .start()
            .map_err(|err| ScabbardStateError(format!("failed to start executor: {}", err)))?;

        Ok(ScabbardState {
            db,
            context_manager,
            executor,
            current_state_root,
            transaction_receipt_store: Arc::new(RwLock::new(TransactionReceiptStore::new(
                Box::new(
                    LmdbOrderedStore::new(receipt_db_path, Some(receipt_db_size))
                        .map_err(|err| ScabbardStateError(err.to_string()))?,
                ),
            ))),
            pending_changes: None,
            event_subscribers: vec![],
            batch_history: BatchHistory::new(),
        })
    }

    fn read_current_state_root(db: &dyn Database) -> Result<Option<String>, ScabbardStateError> {
        db.get_reader()
            .and_then(|reader| reader.index_get(CURRENT_STATE_ROOT_INDEX, b"HEAD"))
            .map(|head| head.map(|bytes| hex::to_hex(&bytes)))
            .map_err(|e| ScabbardStateError(format!("Unable to read HEAD entry: {}", e)))
    }

    fn write_current_state_root(&self) -> Result<(), ScabbardStateError> {
        let current_root_bytes = hex::parse_hex(&self.current_state_root).map_err(|e| {
            ScabbardStateError(format!(
                "The in-memory current state root is invalid: {}",
                e
            ))
        })?;

        let mut writer = self.db.get_writer().map_err(|e| {
            ScabbardStateError(format!(
                "Unable to start write transaction for HEAD entry: {}",
                e
            ))
        })?;

        writer
            .index_put(CURRENT_STATE_ROOT_INDEX, b"HEAD", &current_root_bytes)
            .map_err(|e| ScabbardStateError(format!("Unable to write HEAD entry: {}", e)))?;

        writer
            .commit()
            .map_err(|e| ScabbardStateError(format!("Unable to commit HEAD entry: {}", e)))?;

        Ok(())
    }

    pub fn prepare_change(&mut self, batch: BatchPair) -> Result<String, ScabbardStateError> {
        // Setup the transact scheduler
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let mut scheduler = SerialScheduler::new(
            Box::new(self.context_manager.clone()),
            self.current_state_root.clone(),
        )?;
        scheduler.set_result_callback(Box::new(move |batch_result| {
            if result_tx.send(batch_result).is_err() {
                error!("Unable to send batch result; receiver must have dropped");
            }
        }))?;

        // Add the batch to, finalize, and execute the scheduler
        scheduler.add_batch(batch.clone())?;
        scheduler.finalize()?;
        self.executor
            .execute(scheduler.take_task_iterator()?, scheduler.new_notifier()?)?;

        // Get the results and shutdown the scheduler
        let batch_result = result_rx
            .recv_timeout(std::time::Duration::from_secs(EXECUTION_TIMEOUT))
            .map_err(|_| ScabbardStateError("failed to receive result in reasonable time".into()))?
            .ok_or_else(|| ScabbardStateError("no result returned from executor".into()))?;

        let batch_status = batch_result.clone().into();
        let signature = batch.batch().header_signature();
        self.batch_history
            .update_batch_status(&signature, batch_status);

        let txn_results = batch_result
            .results
            .into_iter()
            .map(|txn_result| match txn_result {
                TransactionExecutionResult::Valid(receipt) => Ok(receipt),
                TransactionExecutionResult::Invalid(invalid_result) => Err(ScabbardStateError(
                    format!("transaction failed: {:?}", invalid_result),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;

        scheduler.shutdown();

        // Save the results and compute the resulting state root
        let state_root = MerkleState::new(self.db.clone()).compute_state_id(
            &self.current_state_root,
            &receipts_into_transact_state_changes(&txn_results),
        )?;
        self.pending_changes = Some((signature.to_string(), txn_results));
        Ok(state_root)
    }

    pub fn commit(&mut self) -> Result<(), ScabbardStateError> {
        match self.pending_changes.take() {
            Some((signature, txn_receipts)) => {
                let state_changes = receipts_into_transact_state_changes(&txn_receipts);
                self.current_state_root = MerkleState::new(self.db.clone())
                    .commit(&self.current_state_root, &state_changes)?;

                self.write_current_state_root()?;

                info!(
                    "committed {} change(s) for new state root {}",
                    state_changes.len(),
                    self.current_state_root,
                );

                let events = txn_receipts
                    .iter()
                    .map(receipt_into_scabbard_state_change_event)
                    .collect::<Vec<_>>();

                self.transaction_receipt_store
                    .write()
                    .map_err(|err| {
                        ScabbardStateError(format!(
                            "transaction receipt store lock poisoned: {}",
                            err
                        ))
                    })?
                    .append(txn_receipts)
                    .map_err(|err| {
                        ScabbardStateError(format!(
                            "failed to add transaction receipts to store: {}",
                            err
                        ))
                    })?;

                for event in events {
                    self.event_subscribers.retain(|subscriber| {
                        match subscriber.handle_event(event.clone()) {
                            Ok(()) => true,
                            Err(StateSubscriberError::Unsubscribe) => false,
                            Err(err @ StateSubscriberError::UnableToHandleEvent(_)) => {
                                error!("{}", err);
                                true
                            }
                        }
                    });
                }

                self.batch_history.commit(&signature);

                Ok(())
            }
            None => Err(ScabbardStateError("no pending changes to commit".into())),
        }
    }

    pub fn rollback(&mut self) -> Result<(), ScabbardStateError> {
        match self.pending_changes.take() {
            Some((_, txn_receipts)) => info!(
                "discarded {} change(s)",
                receipts_into_transact_state_changes(&txn_receipts).len()
            ),
            None => debug!("no changes to rollback"),
        }

        Ok(())
    }

    pub fn batch_history(&mut self) -> &mut BatchHistory {
        &mut self.batch_history
    }

    pub fn get_events_since(&self, event_id: Option<String>) -> Result<Events, ScabbardStateError> {
        Events::new(self.transaction_receipt_store.clone(), event_id)
    }

    pub fn add_subscriber(&mut self, subscriber: Box<dyn StateSubscriber>) {
        self.event_subscribers.push(subscriber);
    }

    pub fn clear_subscribers(&mut self) {
        self.event_subscribers.clear();
    }
}

fn receipts_into_transact_state_changes(
    receipts: &[TransactionReceipt],
) -> Vec<TransactStateChange> {
    receipts
        .iter()
        .flat_map(|receipt| {
            receipt
                .state_changes
                .iter()
                .cloned()
                .map(|change| match change {
                    transact::protocol::receipt::StateChange::Set { key, value } => {
                        TransactStateChange::Set { key, value }
                    }
                    transact::protocol::receipt::StateChange::Delete { key } => {
                        TransactStateChange::Delete { key }
                    }
                })
        })
        .collect::<Vec<_>>()
}

fn receipt_into_scabbard_state_change_event(receipt: &TransactionReceipt) -> StateChangeEvent {
    let state_changes = receipt
        .state_changes
        .iter()
        .cloned()
        .map(|change| match change {
            transact::protocol::receipt::StateChange::Set { key, value } => {
                StateChange::Set { key, value }
            }
            transact::protocol::receipt::StateChange::Delete { key } => StateChange::Delete { key },
        })
        .collect();

    StateChangeEvent {
        id: receipt.transaction_id.clone(),
        state_changes,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateChangeEvent {
    pub id: String,
    pub state_changes: Vec<StateChange>,
}

#[cfg(feature = "events")]
impl ParseBytes<StateChangeEvent> for StateChangeEvent {
    fn from_bytes(bytes: &[u8]) -> Result<StateChangeEvent, ParseError> {
        serde_json::from_slice(bytes)
            .map_err(Box::new)
            .map_err(|err| ParseError::MalformedMessage(err))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum StateChange {
    Set { key: String, value: Vec<u8> },
    Delete { key: String },
}

impl fmt::Display for StateChange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StateChange::Set { key, value } => {
                write!(f, "Set(key: {}, payload_size: {})", key, value.len())
            }
            StateChange::Delete { key } => write!(f, "Delete(key: {})", key),
        }
    }
}

impl fmt::Debug for StateChange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

pub trait StateSubscriber: Send {
    fn handle_event(&self, event: StateChangeEvent) -> Result<(), StateSubscriberError>;
}

#[derive(PartialEq)]
enum EventQuery {
    Fetch(Option<String>),
    Exhausted,
}

/// An iterator that wraps the `TransactionReceiptStore` and returns `StateChangeEvent`s using an
/// in-memory cache.
pub struct Events {
    transaction_receipt_store: Arc<RwLock<TransactionReceiptStore>>,
    query: EventQuery,
    cache: VecDeque<StateChangeEvent>,
}

impl Events {
    fn new(
        transaction_receipt_store: Arc<RwLock<TransactionReceiptStore>>,
        start_id: Option<String>,
    ) -> Result<Self, ScabbardStateError> {
        let mut iter = Events {
            transaction_receipt_store,
            query: EventQuery::Fetch(start_id),
            cache: VecDeque::default(),
        };
        iter.reload_cache()?;
        Ok(iter)
    }

    fn reload_cache(&mut self) -> Result<(), ScabbardStateError> {
        match self.query {
            EventQuery::Fetch(ref start_id) => {
                let transaction_receipt_store =
                    self.transaction_receipt_store.read().map_err(|err| {
                        ScabbardStateError(format!(
                            "transaction receipt store lock poisoned: {}",
                            err
                        ))
                    })?;

                self.cache = if let Some(id) = start_id.as_ref() {
                    transaction_receipt_store.iter_since_id(id.clone())
                } else {
                    transaction_receipt_store.iter()
                }
                .map_err(|err| {
                    ScabbardStateError(format!(
                        "failed to get transaction receipts from store: {}",
                        err
                    ))
                })?
                .take(ITER_CACHE_SIZE)
                .map(|ref receipt| receipt_into_scabbard_state_change_event(receipt))
                .collect::<VecDeque<_>>();

                self.query = self
                    .cache
                    .back()
                    .map(|event| EventQuery::Fetch(Some(event.id.clone())))
                    .unwrap_or(EventQuery::Exhausted);

                Ok(())
            }
            EventQuery::Exhausted => Ok(()),
        }
    }
}

impl Iterator for Events {
    type Item = StateChangeEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cache.is_empty() && self.query != EventQuery::Exhausted {
            if let Err(err) = self.reload_cache() {
                error!("Unable to reload iterator cache: {}", err);
            }
        }
        self.cache.pop_front()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "statusType", content = "message")]
pub enum BatchStatus {
    Unknown,
    Pending,
    Invalid(Vec<InvalidTransaction>),
    Valid(Vec<ValidTransaction>),
    Committed(Vec<ValidTransaction>),
}

impl From<BatchExecutionResult> for BatchStatus {
    fn from(batch_result: BatchExecutionResult) -> Self {
        let mut valid = Vec::new();
        let mut invalid = Vec::new();

        for result in batch_result.results.into_iter() {
            match result {
                TransactionExecutionResult::Valid(r) => {
                    valid.push(ValidTransaction::from(r));
                }
                TransactionExecutionResult::Invalid(r) => {
                    invalid.push(InvalidTransaction::from(r));
                }
            }
        }

        if !invalid.is_empty() {
            BatchStatus::Invalid(invalid)
        } else {
            BatchStatus::Valid(valid)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidTransaction {
    transaction_id: String,
}

impl From<TransactionReceipt> for ValidTransaction {
    fn from(receipt: TransactionReceipt) -> Self {
        Self {
            transaction_id: receipt.transaction_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InvalidTransaction {
    transaction_id: String,
    error_message: String,
    error_data: Vec<u8>,
}

impl From<InvalidTransactionResult> for InvalidTransaction {
    fn from(result: InvalidTransactionResult) -> Self {
        Self {
            transaction_id: result.transaction_id,
            error_message: result.error_message,
            error_data: result.error_data,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchInfo {
    pub id: String,
    pub status: BatchStatus,
    #[serde(skip, default = "SystemTime::now")]
    pub timestamp: SystemTime,
}

impl BatchInfo {
    fn set_status(&mut self, status: BatchStatus) {
        self.status = status;
    }
}

/// BatchHistory keeps track of batches submitted to scabbard
pub struct BatchHistory {
    history: HashMap<String, BatchInfo>,
    limit: usize,
}

impl BatchHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_batch(&mut self, signature: &str) {
        self.history.insert(
            signature.to_string(),
            BatchInfo {
                id: signature.to_string(),
                status: BatchStatus::Pending,
                timestamp: SystemTime::now(),
            },
        );

        if self.history.len() > self.limit {
            self.history
                .clone()
                .into_iter()
                .min_by_key(|(_, v)| v.timestamp)
                .and_then(|(k, _)| self.history.remove(&k));
        }
    }

    fn update_batch_status(&mut self, signature: &str, status: BatchStatus) {
        match self.history.get_mut(signature) {
            Some(ref mut batch) if batch.status == BatchStatus::Pending => {
                batch.set_status(status);
            }
            _ => (),
        };
    }

    fn commit(&mut self, signature: &str) {
        let info = if let Some(info) = self.history.get_mut(signature) {
            info
        } else {
            return;
        };

        if let BatchStatus::Valid(t) = info.status.clone() {
            info.set_status(BatchStatus::Committed(t));
        }
    }

    pub fn get_batch_info(&self, signature: &str) -> BatchInfo {
        if let Some(info) = self.history.get(signature) {
            info.clone()
        } else {
            BatchInfo {
                id: signature.to_string(),
                status: BatchStatus::Unknown,
                timestamp: SystemTime::now(),
            }
        }
    }
}

impl Default for BatchHistory {
    fn default() -> Self {
        Self {
            history: HashMap::new(),
            limit: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMP_DB_SIZE: usize = 1 << 30; // 1024 ** 3

    /// Verify that an empty receipt store returns an empty iterator
    #[test]
    fn empty_event_iterator() {
        let temp_db_path = get_temp_db_path();

        let test_result = std::panic::catch_unwind(|| {
            let transaction_receipt_store =
                Arc::new(RwLock::new(TransactionReceiptStore::new(Box::new(
                    LmdbOrderedStore::new(&temp_db_path, Some(TEMP_DB_SIZE))
                        .expect("Failed to create LMDB store"),
                ))));

            // Test without a specified start
            let all_events = Events::new(transaction_receipt_store.clone(), None)
                .expect("failed to get iterator for all events");
            let all_event_ids = all_events.map(|event| event.id.clone()).collect::<Vec<_>>();
            assert!(
                all_event_ids.is_empty(),
                "All events should have been empty"
            );
        });

        std::fs::remove_file(temp_db_path.as_path()).expect("Failed to remove temp DB file");

        assert!(test_result.is_ok());
    }

    /// Verify that the event iterator works as expected.
    #[test]
    fn event_iterator() {
        let temp_db_path = get_temp_db_path();

        let test_result = std::panic::catch_unwind(|| {
            let receipts = vec![
                TransactionReceipt {
                    state_changes: vec![],
                    events: vec![],
                    data: vec![],
                    transaction_id: "ab".into(),
                },
                TransactionReceipt {
                    state_changes: vec![],
                    events: vec![],
                    data: vec![],
                    transaction_id: "cd".into(),
                },
                TransactionReceipt {
                    state_changes: vec![],
                    events: vec![],
                    data: vec![],
                    transaction_id: "ef".into(),
                },
            ];
            let receipt_ids = receipts
                .iter()
                .map(|receipt| receipt.transaction_id.clone())
                .collect::<Vec<_>>();

            let transaction_receipt_store =
                Arc::new(RwLock::new(TransactionReceiptStore::new(Box::new(
                    LmdbOrderedStore::new(&temp_db_path, Some(TEMP_DB_SIZE))
                        .expect("Failed to create LMDB store"),
                ))));

            transaction_receipt_store
                .write()
                .expect("failed to get write lock")
                .append(receipts.clone())
                .expect("failed to add receipts to store");

            // Test without a specified start
            let all_events = Events::new(transaction_receipt_store.clone(), None)
                .expect("failed to get iterator for all events");
            let all_event_ids = all_events.map(|event| event.id.clone()).collect::<Vec<_>>();
            assert_eq!(all_event_ids, receipt_ids);

            // Test with a specified start
            let some_events = Events::new(
                transaction_receipt_store.clone(),
                Some(receipt_ids[0].clone()),
            )
            .expect("failed to get iterator for some events");
            let some_event_ids = some_events
                .map(|event| event.id.clone())
                .collect::<Vec<_>>();
            assert_eq!(some_event_ids, receipt_ids[1..].to_vec());
        });

        std::fs::remove_file(temp_db_path.as_path()).expect("Failed to remove temp DB file");

        assert!(test_result.is_ok());
    }

    fn get_temp_db_path() -> std::path::PathBuf {
        let mut temp_db_path = std::env::temp_dir();
        let thread_id = std::thread::current().id();
        temp_db_path.push(format!("store-{:?}.lmdb", thread_id));
        temp_db_path
    }
}
