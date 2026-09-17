fn all_derived_worker_rows() -> Vec<DerivedRoute> {
    let mut rows: Vec<DerivedRoute> = Vec::with_capacity(11);
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/commands/{command_id}/complete",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/commands/{command_id}/fail",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/worker/v1/events",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/worker/v1/replication/{worker_id}/position",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::PUT,
            "/_synapse/worker/v1/replication/{worker_id}/{stream_name}",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/tasks/{task_id}/complete",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/tasks/{task_id}/fail",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_synapse/worker/v1/workers/{worker_id}/commands",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/workers/{worker_id}/connect",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/workers/{worker_id}/disconnect",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_synapse/worker/v1/workers/{worker_id}/heartbeat",
            "worker_body",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Worker });
    }
    rows
}