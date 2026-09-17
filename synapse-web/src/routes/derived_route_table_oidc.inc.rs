fn all_derived_oidc_rows() -> Vec<DerivedRoute> {
    let mut rows: Vec<DerivedRoute> = Vec::with_capacity(10);
#[cfg(feature = "builtin-oidc")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/.well-known/jwks.json",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
#[cfg(feature = "builtin-oidc")]
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/.well-known/openid-configuration",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/login/sso/redirect",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/login/sso/userinfo",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/oidc/authorize",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/oidc/callback",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
#[cfg(feature = "builtin-oidc")]
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/oidc/login",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/oidc/logout",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::POST,
            "/_matrix/client/v3/oidc/token",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    {
        let e = RouteEntry::new(
            axum::http::Method::GET,
            "/_matrix/client/v3/oidc/userinfo",
            "oidc",
        )
        ;
        rows.push(DerivedRoute { entry: e, rank: RouteProfile::Oidc });
    }
    rows
}