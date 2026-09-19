fn all_derived_always_rows() -> Vec<DerivedRoute> {
    let mut rows: Vec<DerivedRoute> = Vec::with_capacity(1132);
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/.well-known/jwks.json", "oidc_fallback");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/.well-known/matrix/client", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/.well-known/matrix/server", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/.well-known/matrix/support", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/.well-known/openid-configuration", "oidc_fallback");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_health", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/admin/v1/external_services", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/admin/v1/external_services", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/admin/v1/external_services/health", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/admin/v1/external_services/{as_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/admin/v1/external_services/{as_id}", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/app/v1/ping", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/app/v1/rooms/{alias}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/app/v1/transactions/{as_id}/{txn_id}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/app/v1/users/{user_id}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/app/v1/{as_id}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc2965/auth_issuer",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc2965/auth_metadata",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/unstable/org.matrix.msc3575/sync",
            "sliding_sync",
        )
        .with_rate_limit_exempt(true);
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/unstable/org.matrix.msc4108/rendezvous",
            "msc4108_rendezvous",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}",
            "msc4108_rendezvous",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}",
            "msc4108_rendezvous",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}",
            "msc4108_rendezvous",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}",
            "delayed_events",
        )
        .with_auth("user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc4155/rooms/{room_id}/threads",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/org.matrix.msc4156/threads/subscribed",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
            "sliding_sync",
        )
        .with_rate_limit_exempt(true);
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
            "assembly::create_router",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/account/3pid", "assembly::account_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/account/3pid", "assembly::account_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/3pid/add",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/3pid/bind",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/3pid/delete",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/3pid/email/requestToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/3pid/email/submitToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/3pid/unbind",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/deactivate",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/password",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/password/email/requestToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/account/password/email/submitToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/account/whoami", "assembly::account_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/auth_metadata", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/config/client", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/external_services/health", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/external_services/{service_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/external_services/{service_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/friends", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/check/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/dm/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/friends/dm/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/groups", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/friends/groups", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/friends/groups/{group_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/friends/groups/{group_id}/friends",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/friends/groups/{group_id}/name",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/friends/request", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/request/received", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/friends/request/{user_id}/accept",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/friends/request/{user_id}/cancel",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/friends/request/{user_id}/reject",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/requests/incoming", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/requests/outgoing", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/search", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/friends/search", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/suggestions", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/friends/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/friends/{user_id}/displayname", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/{user_id}/groups", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/{user_id}/info", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/friends/{user_id}/note", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/friends/{user_id}/status", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/friends/{user_id}/status", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/keys/changes", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/claim", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/device_list/update", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/keys/device_signing/requests",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/device_signing/upload", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/keys/device_signing/verify_accept",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/device_signing/verify_cancel",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/device_signing/verify_done",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/device_signing/verify_key_agreement",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/device_signing/verify_mac",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/device_signing/verify_start",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/qr_code/scan", "verification_routes");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/keys/qr_code/show", "verification_routes");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/query", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/keys/rotation/check", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/rotation/check", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/rotation/config", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/keys/rotation/config", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/keys/rotation/history/{device_id}",
            "key_rotation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/rotation/revoke", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/rotation/rotate", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/keys/rotation/status", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/rotation/status", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/signatures", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/signatures/upload", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/upload", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/keys/upload/{device_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/verification/request",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/keys/verification/{transaction_id}",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/keys/verification/{transaction_id}/cancel",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/login/qr_token", "assembly::auth_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/media/config", "assembly::media_config");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/media/download/{server_name}/{media_id}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/media/download/{server_name}/{media_id}/{filename}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/media/preview_url", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/media/thumbnail/{server_name}/{media_id}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/presence/{user_id}/status", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/presence/{user_id}/status", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/presence/{user_id}/status", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/profile/{user_id}",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/profile/{user_id}/avatar_url",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/profile/{user_id}/avatar_url",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/profile/{user_id}/displayname",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/profile/{user_id}/displayname",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/rendezvous", "rendezvous");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/rendezvous/{session_id}", "rendezvous");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rendezvous/{session_id}", "rendezvous");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/rendezvous/{session_id}", "rendezvous");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rendezvous/{session_id}/messages",
            "rendezvous",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rendezvous/{session_id}/messages",
            "rendezvous",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/room_keys/batch_recover", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/export", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/export/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/room_keys/import", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/room_keys/import/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/room_keys/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/room_keys/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/room_keys/keys/{room_id}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/keys/{room_id}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/room_keys/keys/{room_id}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/room_keys/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/room_keys/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/room_keys/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/room_keys/recover", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/room_keys/recover/{version}/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/room_keys/recover/{version}/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/room_keys/recovery/{version}/progress",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/request", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/room_keys/request", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/room_keys/request/{request_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/verify/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/version", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/room_keys/version", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/room_keys/version/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/version/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/room_keys/version/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/room_keys/{version}/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/room_keys/{version}/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/room_keys/{version}/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/room_keys/{version}/keys/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/room_keys/{version}/keys/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/room_keys/{version}/keys/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/room_keys/{version}/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/room_keys/{version}/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/room_keys/{version}/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/rooms/create_private", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/aggregations/{event_id}/{rel_type}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/burn", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/rooms/{room_id}/burn", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/burn/pending",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/context/{event_id}", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/hierarchy", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/keys/distribution", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/relations/{event_id}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/relations/{event_id}/{rel_type}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}",
            "moderation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info",
            "moderation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/score",
            "moderation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/state/m.room.power_levels/",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/rooms/{room_id}/state/m.room.power_levels/",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/summary", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/threads", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/rooms/{room_id}/threads", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/threads/search", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/threads/unread", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/freeze",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/mute",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/read",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/stats",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unfreeze",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/timestamp_to_event", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/rooms/{room_id}/widgets", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config",
            "widget",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v1/sendToDevice/{event_type}/{transaction_id}",
            "e2ee",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v1/sendToDevice/{event_type}/{transaction_id}",
            "e2ee",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/spaces", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/public", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/room/{room_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/room/{room_id}/parents", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/search", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/statistics", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/user", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/spaces/{space_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/spaces/{space_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/children", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/spaces/{space_id}/children", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/spaces/{space_id}/children/{room_id}",
            "space",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/hierarchy", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/hierarchy/v1", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/spaces/{space_id}/invite", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/spaces/{space_id}/join", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/spaces/{space_id}/leave", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/members", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/rooms", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/state", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/summary", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v1/spaces/{space_id}/summary/with_children",
            "space",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/spaces/{space_id}/tree_path", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/sync", "sliding_sync")
            .with_rate_limit_exempt(true);
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/threads", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/threads", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/threads/subscribed", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/threads/unread", "thread");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/user/burn/config", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/user/burn/stats", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/user/mutual_rooms", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/user/{user_id}/appservice", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/voice/config", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/voice/register", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/voice/room/{room_id}/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/voice/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/voice/upload", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/voice/user/{user_id}/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/widgets", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/widgets/sessions/{session_id}", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/widgets/sessions/{session_id}", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v1/widgets/{widget_id}", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/widgets/{widget_id}", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v1/widgets/{widget_id}", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/widgets/{widget_id}/config", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/widgets/{widget_id}/permissions", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/widgets/{widget_id}/permissions", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}",
            "widget",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v1/widgets/{widget_id}/sessions", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v1/widgets/{widget_id}/sessions", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/account/3pid", "assembly::account_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/account/3pid", "assembly::account_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/3pid/add",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/3pid/bind",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/3pid/delete",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/3pid/email/requestToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/3pid/email/submitToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/3pid/unbind",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/deactivate",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/account/guest", "guest");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/account/guest/upgrade", "guest");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/password",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/password/email/requestToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/account/password/email/submitToken",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/account/whoami", "assembly::account_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/admin/room/{room_id}/redact", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/appservice/alias", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/appservice/user", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/auth/{auth_type}/fallback/web",
            "assembly::auth_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/capabilities", "assembly::capabilities");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/createRoom", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/create_dm", "dm");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/delete_devices", "device");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/device_trust", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/device_trust/{device_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/device_verification/request", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/device_verification/respond", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/device_verification/status/{token}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/devices", "device");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/devices/{device_id}", "device");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/devices/{device_id}", "device");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/devices/{device_id}", "device");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/direct", "dm");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/direct/{room_id}", "dm");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/directory/list/room/{room_id}",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/directory/list/room/{room_id}",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/directory/room/{room_alias}",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/directory/room/{room_alias}",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/directory/room/{room_alias}",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/directory/room/{room_id}/alias",
            "assembly::directory_extra",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/directory/room/{room_id}/alias/{room_alias}",
            "assembly::directory_extra",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/directory/room/{room_id}/alias/{room_alias}",
            "assembly::directory_extra",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/events", "sync");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/friends", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/friends", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/friends/check/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/friends/requests/incoming", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/friends/requests/outgoing", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/friends/search", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/friends/search", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/invite/{room_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/join/{room_id_or_alias}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/joined_rooms", "sync");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/keys/backup/secure", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/backup/secure", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/keys/backup/secure/{backup_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/keys/backup/secure/{backup_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/backup/secure/{backup_id}/keys", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/backup/secure/{backup_id}/restore",
            "e2ee",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/backup/secure/{backup_id}/verify",
            "e2ee",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/keys/changes", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/claim", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/device_list/update", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/device_list_updates", "device");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/keys/device_signing/requests",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/device_signing/upload", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/keys/device_signing/verify_accept",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/device_signing/verify_cancel",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/device_signing/verify_done",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/device_signing/verify_key_agreement",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/device_signing/verify_mac",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/device_signing/verify_start",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/keys/history", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/qr_code/scan", "verification_routes");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/keys/qr_code/show", "verification_routes");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/query", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/signatures", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/signatures/upload", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/upload", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/keys/upload/{device_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/verification/request",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/keys/verification/{transaction_id}",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/keys/verification/{transaction_id}/cancel",
            "verification_routes",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/knock/{room_id_or_alias}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/login", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/login", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/login/saml/callback", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/login/saml/callback", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/login/sso/redirect/cas", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/login/sso/redirect/saml", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/login/sso/redirect/saml", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/logout", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/logout/all", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/logout/saml", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/logout/saml/callback", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/media/config", "assembly::media_config");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/my_rooms", "sync");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/notifications", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/notifications/{notification_id}/ack", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/presence/list", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/presence/list", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/presence/list/{user_id}", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/presence/{user_id}/status", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/presence/{user_id}/status", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/presence/{user_id}/status", "presence");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/profile/{user_id}",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/profile/{user_id}/avatar_url",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/profile/{user_id}/avatar_url",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/profile/{user_id}/displayname",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/profile/{user_id}/displayname",
            "assembly::account_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/publicRooms", "assembly::directory_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/publicRooms", "assembly::directory_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/push/devices", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/push/devices", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/push/devices/{device_id}",
            "push_notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/push/send", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushers", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/pushers", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushers/", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/pushers/", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/pushers/set", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushrules", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushrules/", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushrules/global/", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushrules/{scope}", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushrules/{scope}/{kind}", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}",
            "push",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}", "push");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions",
            "push",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
            "push",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
            "push",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/refresh", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/register", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/register", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/register/available", "assembly::auth_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/register/captcha/clean", "captcha");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/register/captcha/send", "captcha");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/register/captcha/status", "captcha");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/register/captcha/verify", "captcha");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/register/email/requestToken",
            "assembly::auth_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/register/email/submitToken",
            "assembly::auth_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/register/guest", "guest");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/room_keys/batch_recover", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/export", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/export/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/room_keys/import", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/room_keys/import/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/room_keys/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/room_keys/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/room_keys/keys/{room_id}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/keys/{room_id}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/room_keys/keys/{room_id}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/room_keys/recover", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/room_keys/recover/{version}/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/room_keys/recover/{version}/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/room_keys/recovery/{version}/progress",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/request", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/room_keys/request", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/room_keys/request/{request_id}", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/verify/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/version", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/room_keys/version", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/room_keys/version/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/version/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/room_keys/version/{version}", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/room_keys/{version}/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/room_keys/{version}/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/room_keys/{version}/keys", "key_backup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/room_keys/{version}/keys/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/room_keys/{version}/keys/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/room_keys/{version}/keys/{room_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}",
            "key_backup",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/create_private", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/typing", "typing");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/account_data/{type}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/account_data/{type}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/aggregations/{event_id}/{rel_type}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/aliases", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/anti_screenshot", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/anti_screenshot", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/ban", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/burn", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/burn", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/burn/pending",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/burn/{event_id}",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voip-tracking")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/call/{call_id}",
            "assembly::voip_tracking",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/capabilities", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/context/{event_id}", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/convert/{event_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/device/{device_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/dm", "dm");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/dm/partner", "dm");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/encrypted_events", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/ephemeral", "ephemeral");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/event/{event_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/event/{event_id}/url", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/event_perspective", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/external_ids", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/forget", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/fragments/{user_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/get_membership_events",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/hierarchy", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/initialSync", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/invite", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/invite_allowlist", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/invite_allowlist", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/invite_blocklist", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/invite_blocklist", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/invites", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/join", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/joined_members", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/keys", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/keys/claim", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/keys/count", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/keys/distribution", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/keys/version", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/keys/{event_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/kick", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/leave", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/members", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/members/recent", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/membership/{user_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/message_queue", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/messages", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/metadata", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/notifications", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/permissions", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/pinned_events", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/pinned_events", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/rooms/{room_id}/pinned_events/{event_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/read_markers", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/read_markers", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/receipts/{receipt_type}/{event_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/reduced_events", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}",
            "relations",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/rendered/", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/report", "moderation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
            "moderation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
            "moderation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/resolve", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/retention", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/room_keys/keys", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/search", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voip-tracking")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.answer/{txn_id}",
            "assembly::voip_tracking",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voip-tracking")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.candidates/{txn_id}",
            "assembly::voip_tracking",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voip-tracking")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.hangup/{txn_id}",
            "assembly::voip_tracking",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voip-tracking")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.invite/{txn_id}",
            "assembly::voip_tracking",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}",
            "reactions",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/service_types", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/sign/{event_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/spaces", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/state", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/state/{event_type}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/state/{event_type}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/state/{event_type}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/sticky_events", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/sticky_events", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/rooms/{room_id}/summary", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/summary", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/summary", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/summary", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/summary/heroes/recalculate",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/summary/members",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/summary/members",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/summary/state",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/summary/stats",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/summary/stats/recalculate",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/summary/sync",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/summary/unread/clear",
            "room_summary",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/sync", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/thread/{event_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/threads/{thread_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/timeline", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/translate/{event_id}",
            "room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/turn_server", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/typing", "typing");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}", "typing");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}", "typing");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}", "typing");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/unban", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/unread_count", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/upgrade", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/vault_data", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/vault_data", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/rooms/{room_id}/verify/{event_id}", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/version", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/rooms/{room_id}/visibility", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/rooms/{room_id}/visibility", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
            "widget",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
            "widget",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send",
            "widget",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/saml/metadata", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/saml/sp_metadata", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/search", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/search_recipients", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/search_rooms", "search");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/security/summary", "e2ee");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}",
            "e2ee",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}",
            "e2ee",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/spaces", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/public", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/room/{room_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/room/{room_id}/parents", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/search", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/statistics", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/user", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/client/v3/spaces/{space_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/spaces/{space_id}", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/children", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/spaces/{space_id}/children", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/spaces/{space_id}/children/{room_id}",
            "space",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/hierarchy", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/hierarchy/v1", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/spaces/{space_id}/invite", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/spaces/{space_id}/join", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/spaces/{space_id}/leave", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/members", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/rooms", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/state", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/summary", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/spaces/{space_id}/summary/with_children",
            "space",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/spaces/{space_id}/tree_path", "space");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/sync", "sync").with_rate_limit_exempt(true);
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/thirdparty/location", "thirdparty");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/thirdparty/location/{protocol}", "thirdparty");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/thirdparty/protocol/{protocol}", "thirdparty");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/thirdparty/protocols", "thirdparty");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/thirdparty/user", "thirdparty");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/thirdparty/user/{protocol}", "thirdparty");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/translate", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/upload/provider", "assembly::upload_provider");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/upload/token", "assembly::upload_provider");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/user/burn/config", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/user/burn/stats", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/user/{user_id}/account_data/", "account_data");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/user/{user_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/user/{user_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/user/{user_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/user/{user_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/user/{user_id}/filter", "account_data");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/client/v3/user/{user_id}/filter", "account_data");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/user/{user_id}/openid/request_token",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/user/{user_id}/openid/request_token",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/user/{user_id}/rooms", "room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            "account_data",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags", "tags");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
            "tags",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
            "tags",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads",
            "thread",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/user/{user_id}/tags", "tags");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/user_directory/list",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/user_directory/profiles/{user_id}",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/user_directory/search",
            "assembly::directory_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/users/{user_id}/report", "moderation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/versions", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/config", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/voice/register", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/room/{room_id}", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/room/{room_id}/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/voice/upload", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/user/{user_id}", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/user/{user_id}/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voice/{media_id}", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/voice/{media_id}/convert", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/voice/{media_id}/optimize", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/voice/{media_id}/transcription", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voip/config", "assembly::voip_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/v3/voip/turnServer", "assembly::voip_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/voip/turnServer", "assembly::voip_compat");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/voip/turnServer/guest",
            "assembly::voip_compat",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "widgets")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v3/widgets/create", "widget");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/client/v4/sync", "sliding_sync")
            .with_rate_limit_exempt(true);
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/client/versions", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/backfill/{room_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/event/{event_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v1/exchange_third_party_invite/{room_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/federation/v1/get_missing_events/{room_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/hierarchy/{room_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v1/invite/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/knock/{room_id}/{user_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/make_leave/{room_id}/{user_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/media/download/{server_name}/{media_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/members/{room_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/members/{room_id}/joined", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/openid/userinfo", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/publicRooms", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/publicRooms", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/query/destination", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/query/directory", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/query/directory/room/{room_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/query/profile", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/query/profile/{user_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/room/{room_id}/{event_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/federation/v1/send/{txn_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/state/{room_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/state_ids/{room_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/thirdparty/invite", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v1/timestamp_to_event/{room_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/user/devices/{user_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/user/keys/claim", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/user/keys/query", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/user/keys/upload", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/version", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v2/invite/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v2/query/{server_name}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/federation/v2/query/{server_name}/{key_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v2/send_join/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v2/send_leave/{room_id}/{event_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v2/server", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v2/user/keys/query", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/key/v2/query", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/key/v2/query/{server_name}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/key/v2/query/{server_name}/{key_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/key/v2/server", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/r0/config", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/r0/delete/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/media/r0/download/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/media/r0/download/{server_name}/{media_id}/{filename}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/r0/preview_url", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/r0/upload", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/media/r1/download/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/media/r1/download/{server_name}/{media_id}/{filename}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/config", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v1/delete/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/download/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/media/v1/download/{server_name}/{media_id}/{filename}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/preview_url", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/quota/alerts", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/quota/check", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/quota/stats", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v1/upload", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v1/upload/chunk", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v1/upload/chunk/cancel", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v1/upload/chunk/complete", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v1/upload/chunk/progress", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v1/upload/chunk/start", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v3/config", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v3/delete/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v3/download/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/media/v3/download/{server_name}/{media_id}/{filename}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/media/v3/download_signed/{server_name}/{media_id}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/media/v3/download_signed/{server_name}/{media_id}/{filename}",
            "media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v3/preview_url", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/media/v3/thumbnail/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/media/v3/upload", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/media/v3/upload/{server_name}/{media_id}", "media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/server_version", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/static/client/login/", "assembly::auth_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/external_services/health", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/vendor/v1/external_services/{service_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/vendor/v1/external_services/{service_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/friends", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/check/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/dm/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/friends/dm/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/groups", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/friends/groups", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e =
            RouteEntry::new(axum::http::Method::DELETE, "/_matrix/vendor/v1/friends/groups/{group_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/vendor/v1/friends/groups/{group_id}/add/{user_id}",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/vendor/v1/friends/groups/{group_id}/friends",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/vendor/v1/friends/groups/{group_id}/name",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/vendor/v1/friends/groups/{group_id}/remove/{user_id}",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/friends/request", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/request/received", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/vendor/v1/friends/request/{user_id}/accept",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/vendor/v1/friends/request/{user_id}/cancel",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/vendor/v1/friends/request/{user_id}/reject",
            "friend_room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/requests/incoming", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/requests/outgoing", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/search", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/friends/search", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/suggestions", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_matrix/vendor/v1/friends/{user_id}", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_matrix/vendor/v1/friends/{user_id}/displayname", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/{user_id}/groups", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/{user_id}/info", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/vendor/v1/friends/{user_id}/note", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/friends/{user_id}/status", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "friends")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/vendor/v1/friends/{user_id}/status", "friend_room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/keys/rotation/check", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/keys/rotation/check", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/keys/rotation/config", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/vendor/v1/keys/rotation/config", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/vendor/v1/keys/rotation/history/{device_id}",
            "key_rotation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/keys/rotation/revoke", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/keys/rotation/rotate", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/keys/rotation/status", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/keys/rotation/status", "key_rotation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/my_rooms", "vendor");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/rooms/{room_id}/burn", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/vendor/v1/rooms/{room_id}/burn", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/vendor/v1/rooms/{room_id}/burn/pending",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}",
            "burn_after_read",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/search_recipients", "vendor");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/search_rooms", "vendor");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_matrix/vendor/v1/user/burn/config", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "burn-after-read")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/user/burn/stats", "burn_after_read");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/config", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/voice/register", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/room/{room_id}", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/room/{room_id}/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/voice/upload", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/user/{user_id}", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/user/{user_id}/stats", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_matrix/vendor/v1/voice/{media_id}", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/voice/{media_id}/convert", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/voice/{media_id}/optimize", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "voice-extended")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_matrix/vendor/v1/voice/{media_id}/transcription", "voice");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/info", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/account/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/account/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/account_data_callbacks", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/account_data_callbacks", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/account_validity", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/account_validity/{user_id}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/account_validity/{user_id}/renew", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/appservices", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/query/alias", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/query/user", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/statistics", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/appservices/{as_id}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/{as_id}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/appservices/{as_id}", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/{as_id}/events", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/appservices/{as_id}/events", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/appservices/{as_id}/namespaces",
            "app_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/appservices/{as_id}/ping", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/{as_id}/state", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/appservices/{as_id}/state", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/appservices/{as_id}/state/{state_key}",
            "app_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/appservices/{as_id}/users", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/appservices/{as_id}/users", "app_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/audit/events", "admin::audit");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/audit/events", "admin::audit");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/audit/events/{event_id}", "admin::audit");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/background_updates", "background_update");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/background_updates", "background_update");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/cleanup_locks",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/count",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/background_updates/next", "background_update");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/pending",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/retry_failed",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/running",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/stats",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/status",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/status/{status}/count",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/background_updates/{job_name}",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/{job_name}",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/{job_name}/cancel",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/{job_name}/complete",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/{job_name}/fail",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/background_updates/{job_name}/history",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/{job_name}/progress",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/background_updates/{job_name}/start",
            "background_update",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/captcha/cleanup", "captcha");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/cas/services", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/cas/services", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/cas/services/{service_id}", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/cas/users/{user_id}/attributes", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/cas/users/{user_id}/attributes", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/cleanup/all", "admin::cleanup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/cleanup/rooms", "admin::cleanup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/cleanup/tokens", "admin::cleanup");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/config", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/event_reports", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/event_reports", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/event_reports/count", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/event_reports/event/{event_id}",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/event_reports/rate_limit/{user_id}",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/event_reports/rate_limit/{user_id}/block",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/event_reports/rate_limit/{user_id}/unblock",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/event_reports/reporter/{reporter_user_id}",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/event_reports/room/{room_id}", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/event_reports/stats", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/event_reports/status/{status}",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/event_reports/status/{status}/count",
            "event_report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/event_reports/{id}", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/event_reports/{id}", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/event_reports/{id}", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/event_reports/{id}/dismiss", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/event_reports/{id}/escalate", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/event_reports/{id}/history", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/event_reports/{id}/resolve", "event_report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/experimental_features", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/external_services", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/external_services", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/external_services/health", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/external_services/{as_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/external_services/{as_id}",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/external_services/{as_id}/health",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/external_services/{as_id}/health/check",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/feature-flags", "feature_flags");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/feature-flags", "feature_flags");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/feature-flags/{flag_key}", "feature_flags");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PATCH, "/_synapse/admin/v1/feature-flags/{flag_key}", "feature_flags");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/federation/blacklist", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/federation/cache", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/federation/cache/clear", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/federation/cache/{key}",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/federation/confirm", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/federation/destinations", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/federation/destinations/{destination}",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/federation/destinations/{destination}",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/federation/destinations/{destination}/reset",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/federation/destinations/{destination}/reset_connection",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/federation/destinations/{destination}/rooms",
            "admin::federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/federation/pending", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/federation/resolve", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/federation/rewrite", "admin::federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/health", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/invite/allowlist", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/invite/blocklist", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/jitsi/config", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/media", "admin::media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/media/quota", "admin::media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/media/{media_id}", "admin::media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/media/{media_id}", "admin::media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/media_callbacks", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/media_callbacks", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/media_callbacks/{callback_type}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/modules", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/modules", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/modules/check_spam", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/modules/check_third_party_rule", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/modules/logs/{module_name}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/modules/spam_check/sender/{sender}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/modules/spam_check/{event_id}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/modules/third_party_rule/{event_id}",
            "module",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/modules/type/{module_type}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/modules/{module_name}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/modules/{module_name}", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/modules/{module_name}/config", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/modules/{module_name}/enable", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/notifications", "admin::notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/notifications", "admin::notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/notifications/active", "admin::notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/notifications/{notification_id}",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/notifications/{notification_id}",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/notifications/{notification_id}",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/notifications/{notification_id}/deactivate",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/password_auth_providers", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/password_auth_providers", "module");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/policy/check", "admin::policy");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/policy/status", "admin::policy");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/purge_history", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/purge_media_cache", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/purge_room", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/push/cleanup", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/push/config", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/push/config", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/push/process", "push_notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/quarantine_media/{media_id}/changes",
            "admin::media",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rate-limit-status", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/register", "admin::register");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/register/nonce", "admin::register");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/registration_tokens", "admin::token");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/registration_tokens", "admin::token");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/registration_tokens/{token}",
            "admin::token",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/registration_tokens/{token}", "admin::token");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/registration_tokens/{token}", "admin::token");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/reports", "admin::report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/reports/{report_id}", "admin::report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/reports/{report_id}", "admin::report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/restart", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/retention/policy", "admin::retention");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/retention/policy", "admin::retention");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/retention/policy/{room_id}",
            "admin::retention",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/retention/policy/{room_id}",
            "admin::retention",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/retention/run", "admin::retention");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/retention/status", "admin::retention");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/room_stats", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/room_stats/{room_id}", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/cleanup", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/search", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/search", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/rooms/{room_id}", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/aliases", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/backfill", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/ban", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/block", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/block", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/delete", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/rooms/{room_id}/forward_extremities",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/kick", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/listings", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/make_admin", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/rooms/{room_id}/make_admin", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/members", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/messages", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/purge_history",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/reports", "admin::report");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}",
            "admin::report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}",
            "admin::report",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/search", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/state", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/token_sync", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}",
            "admin::room",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/rooms/{room_id}/unblock", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/rooms/{room_id}/version", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/saml/config", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/saml/config", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/saml/logout", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/saml/mapping/{name_id}", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/saml/mapping/{name_id}", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/saml/mapping/{name_id}", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/saml/mappings", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "saml-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/saml/metadata/refresh", "saml");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/send_server_notice", "admin::notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/server", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/server_notices", "admin::notification");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/server_notices/{notice_id}",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/server_notices/{notice_id}",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/server_version", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/shutdown_room", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/spaces", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/spaces/{space_id}", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/spaces/{space_id}", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/spaces/{space_id}/rooms", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/spaces/{space_id}/stats", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/spaces/{space_id}/users", "admin::room");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/statistics", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/status", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/telemetry/alerts", "telemetry");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/telemetry/alerts/{alert_id}/ack",
            "telemetry",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/telemetry/attributes", "telemetry");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/telemetry/health", "telemetry");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/telemetry/metrics", "telemetry");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/telemetry/status", "telemetry");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/user_sessions/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/user_sessions/{user_id}/invalidate",
            "admin::user",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/user_stats", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/batch", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/batch_deactivate", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/users/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v1/users/{user_id}/admin", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/{user_id}/deactivate", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users/{user_id}/devices", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/users/{user_id}/devices/delete",
            "admin::user",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/devices/{device_id}",
            "admin::user",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/users/{user_id}/devices/{device_id}/delete",
            "admin::user",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/{user_id}/evict", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/{user_id}/login", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/{user_id}/logout", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v1/users/{user_id}/media", "admin::media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users/{user_id}/media", "admin::media");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/users/{user_id}/notification",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/users/{user_id}/notification",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/admin/v1/users/{user_id}/password", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/users/{user_id}/pushers",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "server-notifications")]
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/pushers/{pushkey}",
            "admin::notification",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/rate_limit",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/users/{user_id}/rate_limit",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/admin/v1/users/{user_id}/rate_limit",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/admin/v1/users/{user_id}/refresh_tokens",
            "admin::token",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}",
            "admin::token",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users/{user_id}/rooms", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/shadow_ban",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/admin/v1/users/{user_id}/shadow_ban",
            "admin::security",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users/{user_id}/stats", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/users/{user_id}/tokens", "admin::token");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::DELETE,
            "/_synapse/admin/v1/users/{user_id}/tokens/{token_id}",
            "admin::token",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/whoami", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/whois/{user_id}", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e =
            RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v1/whois/{user_id}/{device_id}", "admin::server");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v2/users", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/admin/v2/users/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/admin/v2/users/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::PUT, "/_synapse/admin/v2/users/{user_id}", "admin::user");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/external/trendradar/{service_id}/webhook",
            "external_service",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "external-services")]
    {
        let e =
            RouteEntry::new(axum::http::Method::POST, "/_synapse/external/webhook/{service_id}", "external_service");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/federation/v1/get_joining_rules/{room_id}",
            "federation",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/federation/v1/keys/claim", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/federation/v1/keys/query", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/federation/v1/keys/upload", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/federation/v1/query/auth", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/federation/v1/room_auth/{room_id}", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/federation/v2/key/clone", "federation");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/room_summary/v1/summaries", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/room_summary/v1/summaries", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/room_summary/v1/summaries/batch", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/room_summary/v1/updates/process", "room_summary");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/worker/v1/register", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/select/{task_type}", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/statistics", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/statistics/types", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/tasks", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/worker/v1/tasks", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/worker/v1/tasks/claim/{worker_id}", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}",
            "worker",
        );
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/topology", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/topology/validate", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/workers", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/workers/type/{worker_type}", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/_synapse/worker/v1/workers/{worker_id}", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/_synapse/worker/v1/workers/{worker_id}", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/_synapse/worker/v1/workers/{worker_id}/commands", "worker");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/admin/services", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/admin/services", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::DELETE, "/admin/services/{service_id}", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/admin/users/{user_id}/attributes", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::POST, "/admin/users/{user_id}/attributes", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/health", "assembly::create_router");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/login", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/logout", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/p3/serviceValidate", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/proxy", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/proxyValidate", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    #[cfg(feature = "cas-sso")]
    {
        let e = RouteEntry::new(axum::http::Method::GET, "/serviceValidate", "cas");
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Always });
    }
    rows
}
