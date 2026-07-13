//! Room & Sync assembly — room, member, event, summary, space, sync, sliding_sync.

use std::sync::Arc;

use synapse_storage::*;

use crate::auth::RoomAuth;
use crate::container::SharedInfra;

#[derive(Clone)]
pub struct RoomSyncServices {
    pub room_storage: Arc<dyn synapse_storage::room::RoomStoreApi>,
    pub member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    pub event_storage: Arc<dyn synapse_storage::event::EventStoreApi>,
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub room_summary_storage: Arc<dyn synapse_storage::room_summary::RoomSummaryStoreApi>,
    pub relations_storage: Arc<dyn synapse_storage::relations::RelationsStoreApi>,
    pub room_summary_service: Arc<crate::room_summary_service::RoomSummaryService>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Arc<crate::beacon_service::BeaconService>,
    pub room_service: Arc<dyn crate::room::RoomServiceApi>,
    pub sync_service: Arc<dyn crate::sync_service::SyncServiceApi>,
    pub sliding_sync_service: Arc<crate::sliding_sync_service::SlidingSyncService>,
    pub typing_service: Arc<crate::typing_service::TypingService>,
    pub space_storage: Arc<dyn synapse_storage::space::SpaceStoreApi>,
    pub space_service: Arc<crate::space_service::SpaceService>,
    pub relations_service: Arc<crate::relations_service::RelationsService>,
    pub thread_storage: Arc<dyn synapse_storage::thread::ThreadStoreApi>,
    pub thread_service: Arc<crate::thread_service::ThreadService>,
    pub room_tag_storage: Arc<dyn synapse_storage::room_tag::RoomTagStoreApi>,
}

impl RoomSyncServices {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        infra: &SharedInfra,
        room_auth: &Arc<dyn RoomAuth>,
        validator: &Arc<synapse_common::validation::Validator>,
        presence_storage: &Arc<dyn synapse_storage::presence::PresenceStoreApi>,
        to_device_storage: &synapse_e2ee::to_device::ToDeviceStorage,
        member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
        event_broadcaster: Arc<synapse_federation::event_broadcaster::EventBroadcaster>,
        app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
        key_rotation_manager: Arc<synapse_federation::KeyRotationManager>,
        federation_client: Arc<dyn synapse_federation::client_api::FederationClientApi>,
        sticky_event_storage: Arc<dyn synapse_storage::sticky_event::StickyEventStoreApi>,
    ) -> Self {
        let server_name_for_storage = infra.config.server.get_server_name().to_string();
        let room_storage: Arc<dyn synapse_storage::room::RoomStoreApi> = Arc::new(RoomStorage::new(&infra.pool));
        let event_storage_concrete = Arc::new(EventStorage::new(&infra.pool, server_name_for_storage));
        let event_storage: Arc<dyn synapse_storage::event::EventStoreApi> = event_storage_concrete.clone();
        let event_reader: Arc<dyn synapse_storage::event::EventReader> = event_storage_concrete.clone();
        let event_writer: Arc<dyn synapse_storage::event::EventWriter> = event_storage_concrete.clone();
        let device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi> =
            Arc::new(DeviceStorage::new(&infra.pool));
        let relations_storage = Arc::new(synapse_storage::relations::RelationsStorage::new(&infra.pool));
        let room_summary_storage: Arc<dyn synapse_storage::room_summary::RoomSummaryStoreApi> =
            Arc::new(synapse_storage::room_summary::RoomSummaryStorage::new(&infra.pool));
        let room_tag_storage = Arc::new(synapse_storage::room_tag::RoomTagStorage::new(infra.pool.clone()));

        let room_summary_service = Arc::new(crate::room_summary_service::RoomSummaryService::new(
            room_summary_storage.clone(),
            event_storage.clone(),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        #[cfg(feature = "beacons")]
        let beacon_storage: Arc<dyn synapse_storage::beacon::BeaconStoreApi> =
            Arc::new(BeaconStorage::new(infra.pool.clone()));
        #[cfg(feature = "beacons")]
        let beacon_service = Arc::new(crate::beacon_service::BeaconService::new(beacon_storage, infra.cache.clone()));

        let room_service = Arc::new(crate::room_service::RoomService::new(crate::room_service::RoomServiceConfig {
            room_storage: room_storage.clone(),
            member_storage: member_storage.clone(),
            event_storage: event_storage.clone(),
            event_reader: Some(event_reader.clone()),
            room_tag_storage: room_tag_storage.clone(),
            user_storage: Arc::new(UserStorage::new(&infra.pool, infra.cache.clone())),
            room_auth: room_auth.clone(),
            room_summary_service: room_summary_service.clone(),
            validator: validator.clone(),
            server_name: infra.config.server.name.clone(),
            task_queue: infra.task_queue.clone(),
            relations_storage: relations_storage.clone(),
            event_broadcaster: Some(event_broadcaster),
            app_service_manager: Some(app_service_manager),
            key_rotation_manager: Some(key_rotation_manager),
            federation_client: Some(federation_client),
            #[cfg(feature = "beacons")]
            beacon_service: Some(beacon_service.clone()),
            #[cfg(not(feature = "beacons"))]
            beacon_service: None,
            sticky_event_storage,
        }));

        let sync_room_account_data_storage = RoomAccountDataStorage::new(&infra.pool);
        let sync_account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi> =
            Arc::new(synapse_storage::account_data::AccountDataStorage::new(&infra.pool));
        let sync_device_key_storage = synapse_e2ee::device_keys::DeviceKeyStorage::new(&infra.pool);
        let sync_key_rotation_storage = synapse_e2ee::key_rotation::KeyRotationStorage::new(infra.pool.clone());
        let sync_service =
            Arc::new(crate::sync_service::SyncService::from_deps(crate::sync_service::SyncServiceDeps {
                presence_storage: presence_storage.clone(),
                member_storage: member_storage.clone(),
                event_storage: event_storage.clone(),
                event_reader: event_reader.clone(),
                room_storage: room_storage.clone(),
                room_account_data_storage: sync_room_account_data_storage,
                account_data_storage: sync_account_data_storage,
                filter_storage: FilterStorage::new(&infra.pool),
                device_storage: device_storage.clone(),
                device_key_storage: sync_device_key_storage.clone(),
                key_rotation_storage: sync_key_rotation_storage,
                to_device_storage: to_device_storage.clone(),
                metrics: infra.metrics.clone(),
                performance: infra.config.performance.clone(),
            }));

        let typing_service = Arc::new(crate::typing_service::TypingService::new(infra.cache.clone()));

        let sliding_sync_storage = Arc::new(synapse_storage::sliding_sync::SlidingSyncStorage::new(infra.pool.clone()));
        let sliding_sync_service = Arc::new(crate::sliding_sync_service::SlidingSyncService::new(
            sliding_sync_storage,
            infra.cache.clone(),
            event_storage.clone(),
            event_reader.clone(),
            sync_device_key_storage,
            typing_service.clone(),
            presence_storage.clone(),
            member_storage.clone(),
            device_storage.clone(),
            to_device_storage.clone(),
            infra.metrics.clone(),
            infra.config.performance.clone(),
        ));

        let space_storage: Arc<dyn synapse_storage::space::SpaceStoreApi> = Arc::new(SpaceStorage::new(&infra.pool));
        let space_service = Arc::new(crate::space_service::SpaceService::new(
            space_storage.clone(),
            room_storage.clone(),
            infra.config.server.name.clone(),
        ));

        let relations_service = Arc::new(crate::relations_service::RelationsService::new(
            relations_storage.clone(),
            infra.config.server.server_name.clone().unwrap_or_default(),
        ));

        let thread_storage: Arc<dyn synapse_storage::thread::ThreadStoreApi> =
            Arc::new(synapse_storage::thread::ThreadStorage::new(&infra.pool));
        let thread_service = Arc::new(crate::thread_service::ThreadService::new(thread_storage.clone()));

        Self {
            room_storage,
            member_storage,
            event_storage,
            event_reader,
            event_writer,
            room_summary_storage,
            relations_storage,
            room_summary_service,
            #[cfg(feature = "beacons")]
            beacon_service,
            room_service,
            sync_service,
            sliding_sync_service,
            typing_service,
            space_storage,
            space_service,
            relations_service,
            thread_storage,
            thread_service,
            room_tag_storage,
        }
    }
}
