use crate::web::routes::{handlers::search, AppState};
use axum::Router;

pub fn create_search_router(state: AppState) -> Router<AppState> {
    search::create_search_router(state)
}
