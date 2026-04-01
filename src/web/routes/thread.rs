use crate::web::routes::{handlers::thread, AppState};
use axum::Router;

pub fn create_thread_routes(state: AppState) -> Router<AppState> {
    thread::create_thread_routes(state)
}
