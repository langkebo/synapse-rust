use axum::{routing::{get, post}, Router};

#[tokio::main]
async fn main() {
    let app1 = Router::<()>::new()
        .route("/sync", get(|| async { "get" }));
    let app2 = Router::<()>::new()
        .route("/sync", post(|| async { "post" }));
        
    let app = app1.merge(app2);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    println!("Router built successfully!");
}
